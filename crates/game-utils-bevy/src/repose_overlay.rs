//! Fullscreen screen-effects overlay for repose (`repose` feature).
//!
//! Ports the flash-white and circle-wipe post effects to a
//! [`WgpuCallback`][repose_render_wgpu::WgpuCallback]: one fullscreen
//! triangle, blended over the frame. Chromatic aberration is NOT
//! portable here: it samples the rendered frame, which an in-pass
//! callback cannot read.
//!
//! Snapshot pattern (signals are `!Send`, callbacks are `Send`): read
//! game state at composition time into [`ScreenEffectsOverlay`] and
//! embed it fullscreen. Skip embedding while [`ScreenEffectsOverlay::is_idle`].

use bytemuck::{Pod, Zeroable};
use repose_core::PaintCallbackInfo;
use repose_render_wgpu::{CallbackResources, ScreenDescriptor, WgpuCallback};

const SHADER: &str = r#"
struct Settings {
    flash_amount: f32,
    circle_wipe_progress: f32,
    circle_wipe_direction: f32,
    _pad: f32,
};

@group(0) @binding(0) var<uniform> s: Settings;

struct VsOut {
    @builtin(position) pos: vec4f,
    @location(0) uv: vec2f,
};

@vertex
fn vs(@builtin(vertex_index) i: u32) -> VsOut {
    let x = f32(i / 2u) * 4.0 - 1.0;
    let y = f32(i % 2u) * 4.0 - 1.0;
    var out: VsOut;
    out.pos = vec4f(x, y, 0.0, 1.0);
    out.uv = vec2f((x + 1.0) * 0.5, (1.0 - y) * 0.5);
    return out;
}

@fragment
fn fs(in: VsOut) -> @location(0) vec4f {
    if (s.circle_wipe_progress > 0.0) {
        let radius = 0.70710678 * s.circle_wipe_progress;
        let inside = distance(in.uv, vec2f(0.5, 0.5)) <= radius;
        if ((s.circle_wipe_direction > 0.0) == inside) {
            return vec4f(0.0, 0.0, 0.0, 1.0);
        }
    }
    return vec4f(1.0, 1.0, 1.0, s.flash_amount);
}
"#;

/// Plain uniform block (16 bytes, Pod for buffer writes).
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct OverlayUniform {
    flash: f32,
    wipe_progress: f32,
    wipe_dir: f32,
    _pad: f32,
}

/// Frozen frame inputs. Snapshot at composition time.
#[derive(Clone, Copy, Default, Debug)]
pub struct ScreenEffectsOverlay {
    /// White overlay alpha (0 = off).
    pub flash: f32,
    /// Wipe radius 0..1 (0 = off).
    pub wipe_progress: f32,
    /// True = black grows outward (expand), false = shrinks (contract).
    pub wipe_expand: bool,
}

impl ScreenEffectsOverlay {
    pub fn new(flash: f32, wipe_progress: f32, wipe_expand: bool) -> Self {
        Self {
            flash: flash.clamp(0.0, 1.0),
            wipe_progress: wipe_progress.clamp(0.0, 1.0),
            wipe_expand,
        }
    }

    /// True when nothing would draw (skip embedding).
    pub fn is_idle(&self) -> bool {
        self.flash <= 0.0 && self.wipe_progress <= 0.0
    }

    fn uniform(&self) -> OverlayUniform {
        OverlayUniform {
            flash: self.flash,
            wipe_progress: self.wipe_progress,
            wipe_dir: if self.wipe_expand { 1.0 } else { -1.0 },
            _pad: 0.0,
        }
    }
}

#[derive(Default)]
struct OverlayPipes {
    pipeline: Option<wgpu::RenderPipeline>,
    ubuf: Option<wgpu::Buffer>,
    bind: Option<wgpu::BindGroup>,
    key: Option<(wgpu::TextureFormat, u32)>,
}

fn ensure_pipes(
    device: &wgpu::Device,
    pipes: &mut OverlayPipes,
    format: wgpu::TextureFormat,
    samples: u32,
) {
    if pipes.pipeline.is_some() && pipes.key == Some((format, samples)) {
        return;
    }
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("game-utils-screen-effects"),
        source: wgpu::ShaderSource::Wgsl(SHADER.into()),
    });
    let ubuf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("game-utils-screen-effects-ubo"),
        size: size_of::<OverlayUniform>() as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("game-utils-screen-effects-bgl"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
    });
    let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("game-utils-screen-effects-bg"),
        layout: &layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: ubuf.as_entire_binding(),
        }],
    });
    let pipe_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("game-utils-screen-effects-layout"),
        bind_group_layouts: &[Some(&layout)],
        immediate_size: 0,
    });
    pipes.pipeline = Some(
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("game-utils-screen-effects"),
            layout: Some(&pipe_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            // Must match the UI pass attachment (see resims-view3d): the
            // overlay draws inside repose's pass, depth ops disabled.
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth24PlusStencil8,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::Always),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: samples,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview_mask: None,
            cache: None,
        }),
    );
    pipes.ubuf = Some(ubuf);
    pipes.bind = Some(bind);
    pipes.key = Some((format, samples));
}

impl WgpuCallback for ScreenEffectsOverlay {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _encoder: &mut wgpu::CommandEncoder,
        screen: &ScreenDescriptor,
        resources: &mut CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        let pipes = resources.get_or_insert_with::<OverlayPipes>();
        ensure_pipes(device, pipes, screen.target_format, screen.sample_count);
        if self.is_idle() {
            return Vec::new();
        }
        queue.write_buffer(
            pipes.ubuf.as_ref().unwrap(),
            0,
            bytemuck::bytes_of(&self.uniform()),
        );
        Vec::new()
    }

    fn paint(
        &self,
        _info: PaintCallbackInfo,
        rpass: &mut wgpu::RenderPass,
        resources: &CallbackResources,
    ) {
        if self.is_idle() {
            return;
        }
        let Some(pipes) = resources.get::<OverlayPipes>() else {
            return;
        };
        let (Some(pipeline), Some(bind)) = (pipes.pipeline.as_ref(), pipes.bind.as_ref()) else {
            return;
        };
        // The renderer sized the viewport to our layout rect; the bare
        // triangle fills exactly that rect (embed fullscreen for wipe).
        rpass.set_pipeline(pipeline);
        rpass.set_bind_group(0, bind, &[]);
        rpass.draw(0..3, 0..1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_detection() {
        assert!(ScreenEffectsOverlay::default().is_idle());
        assert!(!ScreenEffectsOverlay::new(0.5, 0.0, true).is_idle());
        assert!(!ScreenEffectsOverlay::new(0.0, 0.3, false).is_idle());
    }

    #[test]
    fn constructor_clamps_and_maps() {
        let o = ScreenEffectsOverlay::new(2.0, -1.0, false);
        assert_eq!((o.flash, o.wipe_progress), (1.0, 0.0));
        assert_eq!(o.uniform().wipe_dir, -1.0);
        assert_eq!(size_of::<OverlayUniform>(), 16);
    }
}
