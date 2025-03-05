use std::{
    iter,
    sync::Arc
};

use winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    error::EventLoopError,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    window::{Window, WindowId},
};

use wgpu::{
    Backends,
    BlendState,
    Color,
    ColorTargetState,
    ColorWrites,
    CommandEncoderDescriptor,
    Device,
    DeviceDescriptor,
    Face,
    Features,
    FragmentState,
    FrontFace,
    Instance,
    InstanceDescriptor,
    Limits,
    LoadOp,
    MultisampleState,
    Operations,
    PipelineCompilationOptions,
    PipelineLayoutDescriptor,
    PolygonMode,
    PowerPreference,
    PrimitiveState,
    PrimitiveTopology,
    Queue,
    RenderPassColorAttachment,
    RenderPassDescriptor,
    RenderPipeline,
    RenderPipelineDescriptor,
    RequestAdapterOptions,
    StoreOp,
    Surface,
    SurfaceConfiguration,
    SurfaceError,
    TextureUsages,
    TextureViewDescriptor,
    VertexState,
    include_wgsl,
};

use pollster::block_on;

pub struct Settings {
    pub bg_color: Color,
}

pub struct State {
    surface: Surface<'static>,
    device: Device,
    queue: Queue,
    config: SurfaceConfiguration,
    size: PhysicalSize<u32>,
    window: Arc<Window>,
    render_pipeline: RenderPipeline,
    settings: Settings,
}

#[derive(Default)]
pub struct RayTracer {
    state: Option<State>,
}

impl State {
    fn new(window: Arc<Window>) -> Self {
        let size = window.inner_size();

        // The instance is a handle to our GPU
        // Backends::all => Vulkan + Metal + DX12 + Browser WebGPU
        let instance = Instance::new(&InstanceDescriptor {
            #[cfg(not(target_arch = "wasm32"))]
            backends: Backends::PRIMARY,
            #[cfg(target_arch = "wasm32")]
            backends: Backends::BROWSER_WEBGPU,
            ..Default::default()
        });

        let surface = instance.create_surface(window.clone()).unwrap();

        let adapter = block_on(instance.request_adapter(&RequestAdapterOptions {
            power_preference: PowerPreference::default(),
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        })).unwrap();

        let (device, queue) = block_on(adapter.request_device(&DeviceDescriptor {
            required_features: Features::empty(),
            required_limits: if cfg!(target_arch = "wasm32") {
                Limits::downlevel_webgl2_defaults()
            } else {
                Limits::default()
            },
            label: None,
            memory_hints: Default::default(),
        }, None)).unwrap();

        let surface_caps = surface.get_capabilities(&adapter);

        let surface_format = surface_caps.formats.iter()
            .find(|format| format.is_srgb())
            .copied()
            .unwrap_or(surface_caps.formats[0]);

        let config = SurfaceConfiguration {
            usage: TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width,
            height: size.height,
            present_mode: surface_caps.present_modes[0],
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };

        let shader = device.create_shader_module(include_wgsl!("shader.wgsl"));

        let render_pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("Render Pipeline Layout"),
            bind_group_layouts: &[],
            push_constant_ranges: &[],
        });

        let render_pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("Render Pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: PipelineCompilationOptions::default(),
            },
            fragment: Some(FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(ColorTargetState {
                    format: config.format,
                    blend: Some(BlendState::REPLACE),
                    write_mask: ColorWrites::ALL,
                })],
                compilation_options: PipelineCompilationOptions::default(),
            }),
            primitive: PrimitiveState {
                topology: PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: FrontFace::Ccw,
                cull_mode: Some(Face::Back),
                polygon_mode: PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache: None,
        });

        let settings = Settings {
            bg_color: Color {
                r: 0.1,
                g: 0.2,
                b: 0.3,
                a: 1.0,
            },
        };

        Self { surface, device, queue, config, size, window, render_pipeline, settings }
    }

    fn resize(&mut self, new_size: PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.size = new_size;
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.surface.configure(&self.device, &self.config);
        }
    }

    fn input(&mut self, event: &WindowEvent) -> bool {
        match event {
            WindowEvent::CursorMoved {
                device_id: _,
                position
            } => {
                self.settings.bg_color.r = position.x / self.size.width as f64;
                self.settings.bg_color.g = position.y / self.size.height as f64;
                true
            }
            _ => false,
        }
    }

    fn update(&mut self) {
        // Update the state of the application
    }

    fn render(&mut self) -> Result<(), SurfaceError> {
        let output = self.surface.get_current_texture()?;
        let view = output.texture.create_view(&TextureViewDescriptor::default());
        let mut encoder = self.device.create_command_encoder(&CommandEncoderDescriptor {
            label: Some("Render Encoder"),
        });
        let mut render_pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("Render Pass"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Clear(self.settings.bg_color),
                    store: StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            occlusion_query_set: None,
            timestamp_writes: None,
        });
        render_pass.set_pipeline(&self.render_pipeline);
        render_pass.draw(0..3, 0..1);
        drop(render_pass);
        self.queue.submit(iter::once(encoder.finish()));
        output.present();

        Ok(())
    }
}

impl ApplicationHandler for RayTracer {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window = Arc::new(event_loop.create_window(Window::default_attributes()).unwrap());
        self.state = Some(State::new(window));
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        if self.get_state().input(&event) {
            return;
        }
        match event {
            WindowEvent::CloseRequested => {
                println!("The close button was pressed; stopping");
                event_loop.exit();
            },
            WindowEvent::RedrawRequested => {
                if self.state.is_none() {
                    return;
                }
                self.get_window().request_redraw();
                let state = self.get_state();
                state.update();
                match state.render() {
                    Ok(_) => (),
                    Err(SurfaceError::Lost | SurfaceError::Outdated) => {
                        state.resize(state.size);
                    },
                    Err(SurfaceError::OutOfMemory | SurfaceError::Other) => {
                        log::error!("OutOfMemory");
                        event_loop.exit();
                    },
                    Err(SurfaceError::Timeout) => {
                        log::warn!("Surface timeout");
                    }
                }
            }
            WindowEvent::Resized(physical_size) => {
                self.get_state().resize(physical_size);
            }
            _ => (),
        }
    }
}

impl RayTracer {
    pub fn run(&mut self) -> Result<(), EventLoopError> {
        let event_loop = EventLoop::new().unwrap();
        //event_loop.set_control_flow(ControlFlow::Poll);
        event_loop.run_app(self)
    }

    pub fn get_window(&self) -> Arc<Window> {
        self.state.as_ref().unwrap().window.clone()
    }

    pub fn get_state(&mut self) -> &mut State {
        self.state.as_mut().unwrap()
    }
}
