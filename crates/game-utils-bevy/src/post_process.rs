use std::collections::HashMap;

use bevy::{
    asset::{embedded_asset, load_embedded_asset},
    core_pipeline::{
        Core2dSystems, Core3dSystems, FullscreenShader,
        schedule::{Core2d, Core3d},
    },
    prelude::*,
    render::{
        RenderApp, RenderStartup,
        extract_component::{
            ComponentUniforms, DynamicUniformIndex, ExtractComponent, ExtractComponentPlugin,
            UniformComponentPlugin,
        },
        render_resource::{
            binding_types::{sampler, texture_2d, uniform_buffer},
            *,
        },
        renderer::{RenderContext, RenderDevice, ViewQuery},
        view::ViewTarget,
    },
    state::state::FreelyMutableState,
};

use crate::screen_effects::{ChromaticAberration, FlashWhite};
use crate::transitions::{CircleWipeDirection, Transition, TransitionKind};

#[derive(Component, Clone, Copy, ExtractComponent, ShaderType)]
#[extract_app(RenderApp)]
pub struct ScreenEffectSettings {
    pub chromatic_intensity: f32,
    pub flash_amount: f32,
    pub circle_wipe_progress: f32,
    pub circle_wipe_direction: f32,
}

impl Default for ScreenEffectSettings {
    fn default() -> Self {
        Self {
            chromatic_intensity: 0.0,
            flash_amount: 0.0,
            circle_wipe_progress: 0.0,
            circle_wipe_direction: 1.0,
        }
    }
}

#[derive(Resource)]
struct ScreenEffectPipeline {
    layout: BindGroupLayoutDescriptor,
    sampler: Sampler,
    pipelines: HashMap<TextureFormat, CachedRenderPipelineId>,
    shader: Handle<Shader>,
    vertex: VertexState,
}

#[derive(Default)]
struct PostProcessBindGroupCache {
    cached: Option<(TextureViewId, BindGroup)>,
}

pub struct ScreenEffectsPostProcessPlugin;

impl Plugin for ScreenEffectsPostProcessPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "shaders/screen_effects.wgsl");

        app.add_plugins((
            ExtractComponentPlugin::<ScreenEffectSettings>::default(),
            UniformComponentPlugin::<ScreenEffectSettings>::default(),
        ));

        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };

        render_app
            .add_systems(RenderStartup, init_screen_effect_pipeline)
            .add_systems(
                Core2d,
                run_screen_effects.in_set(Core2dSystems::PostProcess),
            )
            .add_systems(
                Core3d,
                run_screen_effects.in_set(Core3dSystems::PostProcess),
            );
    }
}

fn init_screen_effect_pipeline(
    mut commands: Commands,
    render_device: Res<RenderDevice>,
    asset_server: Res<AssetServer>,
    fullscreen_shader: Res<FullscreenShader>,
    pipeline_cache: Res<PipelineCache>,
) {
    let layout = BindGroupLayoutDescriptor::new(
        "screen_effect_bind_group_layout",
        &BindGroupLayoutEntries::sequential(
            ShaderStages::FRAGMENT,
            (
                texture_2d(TextureSampleType::Float { filterable: true }),
                sampler(SamplerBindingType::Filtering),
                uniform_buffer::<ScreenEffectSettings>(true),
            ),
        ),
    );

    let sampler = render_device.create_sampler(&SamplerDescriptor::default());
    let shader = load_embedded_asset!(asset_server.as_ref(), "shaders/screen_effects.wgsl");
    let vertex_state = fullscreen_shader.to_vertex_state();

    // Pre-queue common formats (LDR + HDR). Additional formats are lazily queued.
    let mut pipelines = HashMap::new();
    for format in [
        TextureFormat::Rgba8UnormSrgb,
        TextureFormat::Bgra8UnormSrgb,
        TextureFormat::Rgba16Float,
        TextureFormat::Rgba8Unorm,
        TextureFormat::Bgra8Unorm,
    ] {
        let pipeline_id = pipeline_cache.queue_render_pipeline(RenderPipelineDescriptor {
            label: Some(format!("screen_effect_pipeline_{:?}", format).into()),
            layout: vec![layout.clone()],
            vertex: vertex_state.clone(),
            fragment: Some(FragmentState {
                shader: shader.clone(),
                targets: vec![Some(ColorTargetState {
                    format,
                    blend: None,
                    write_mask: ColorWrites::ALL,
                })],
                ..default()
            }),
            ..default()
        });
        pipelines.insert(format, pipeline_id);
    }

    commands.insert_resource(ScreenEffectPipeline {
        layout,
        sampler,
        pipelines,
        shader,
        vertex: vertex_state,
    });
}

fn run_screen_effects(
    view: ViewQuery<(&ViewTarget, &DynamicUniformIndex<ScreenEffectSettings>)>,
    pipeline_res: Option<ResMut<ScreenEffectPipeline>>,
    pipeline_cache: Res<PipelineCache>,
    settings_uniforms: Res<ComponentUniforms<ScreenEffectSettings>>,
    mut cache: Local<PostProcessBindGroupCache>,
    mut ctx: RenderContext,
) {
    let Some(mut pipeline) = pipeline_res else {
        return;
    };
    let (view_target, settings_index) = view.into_inner();

    let format = view_target.main_texture_format();
    let layout_cloned = pipeline.layout.clone();
    let shader_cloned = pipeline.shader.clone();
    let vertex_cloned = pipeline.vertex.clone();
    let pipeline_id = *pipeline.pipelines.entry(format).or_insert_with(|| {
        pipeline_cache.queue_render_pipeline(RenderPipelineDescriptor {
            label: Some(format!("screen_effect_pipeline_{:?}", format).into()),
            layout: vec![layout_cloned.clone()],
            vertex: vertex_cloned.clone(),
            fragment: Some(FragmentState {
                shader: shader_cloned.clone(),
                targets: vec![Some(ColorTargetState {
                    format,
                    blend: None,
                    write_mask: ColorWrites::ALL,
                })],
                ..default()
            }),
            ..default()
        })
    });

    let Some(render_pipeline) = pipeline_cache.get_render_pipeline(pipeline_id) else {
        return;
    };
    let Some(settings_binding) = settings_uniforms.uniforms().binding() else {
        return;
    };

    let post_process = view_target.post_process_write();

    let bind_group = match &mut cache.cached {
        Some((texture_id, bind_group)) if post_process.source.id() == *texture_id => bind_group,
        cached => {
            let bind_group = ctx.render_device().create_bind_group(
                "screen_effect_bind_group",
                &pipeline_cache.get_bind_group_layout(&pipeline.layout),
                &BindGroupEntries::sequential((
                    post_process.source,
                    &pipeline.sampler,
                    settings_binding.clone(),
                )),
            );
            let (_, bind_group) = cached.insert((post_process.source.id(), bind_group));
            bind_group
        }
    };

    let mut render_pass = ctx
        .command_encoder()
        .begin_render_pass(&RenderPassDescriptor {
            label: Some("screen_effect_pass"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: post_process.destination,
                depth_slice: None,
                resolve_target: None,
                ops: Operations::default(),
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });

    render_pass.set_pipeline(render_pipeline);
    render_pass.set_bind_group(0, bind_group, &[settings_index.index()]);
    render_pass.draw(0..3, 0..1);
}

/// Inserts [`ScreenEffectSettings`] on any camera that doesn't have it yet.
pub fn ensure_screen_effect_settings(
    mut commands: Commands,
    q: Query<
        Entity,
        (
            Or<(With<Camera2d>, With<Camera3d>)>,
            Without<ScreenEffectSettings>,
        ),
    >,
) {
    for e in &q {
        commands.entity(e).insert(ScreenEffectSettings::default());
    }
}

pub fn sync_post_process_settings<S: FreelyMutableState>(
    chroma: Res<ChromaticAberration>,
    flash: Res<FlashWhite>,
    transition: Res<Transition<S>>,
    mut q: Query<&mut ScreenEffectSettings>,
) {
    for mut s in &mut q {
        s.chromatic_intensity = chroma.0;
        s.flash_amount = flash.amount;
        s.circle_wipe_progress = transition.circle_progress;
        s.circle_wipe_direction = match transition.kind {
            TransitionKind::CircleWipe(CircleWipeDirection::Expand) => 1.0,
            TransitionKind::CircleWipe(CircleWipeDirection::Contract) => -1.0,
            TransitionKind::Fade => 1.0,
            TransitionKind::Custom(_) => 1.0,
        };
    }
}
