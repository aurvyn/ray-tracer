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
    util::{
        BufferInitDescriptor,
        DeviceExt,
    },
    Backends,
    BindGroup,
    BindGroupDescriptor,
    BindGroupEntry,
    BindGroupLayoutDescriptor,
    BindGroupLayoutEntry,
    BindingType,
    BlendState,
    Buffer,
    BufferAddress,
    BufferBindingType,
    BufferUsages,
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
    ShaderStages,
    StoreOp,
    Surface,
    SurfaceConfiguration,
    SurfaceError,
    TextureUsages,
    TextureViewDescriptor,
    VertexAttribute,
    VertexBufferLayout,
    VertexState,
    VertexStepMode,
    include_wgsl,
    vertex_attr_array,
};

use pollster::block_on;

const GLTF_PATH: &str = "res/triangle.gltf";

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
    vertex_buffer: Buffer,
    material_bind_group: BindGroup,
    num_vertices: u32,
    settings: Settings,
}

#[derive(Default)]
pub struct RayTracer {
    state: Option<State>,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Vec3 {
    x: f32,
    y: f32,
    z: f32,
}

macro_rules! vec3 {
    [$x:expr, $y:expr, $z:expr] => {
        Vec3 { x: $x, y: $y, z: $z }
    };
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Material {
    ambient: [f32; 4],
    diffuse: [f32; 4],
    specular: [f32; 4],
}

trait Desc {
    const ATTRIBS: [VertexAttribute; 1];
    fn desc() -> VertexBufferLayout<'static>;
}

impl Desc for Vec3 {
    const ATTRIBS: [VertexAttribute; 1] = vertex_attr_array![0 => Float32x3];

    fn desc() -> VertexBufferLayout<'static> {
        VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as BufferAddress,
            step_mode: VertexStepMode::Vertex,
            attributes: &Self::ATTRIBS,
        }
    }
}

impl State {
    fn new(window: Arc<Window>, vertices: Vec<Vec3>, materials: Vec<Material>) -> Self {
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

        let material_buffer = device.create_buffer_init(&BufferInitDescriptor {
            label: Some("Material buffer"),
            contents: bytemuck::cast_slice(&materials),
            usage: BufferUsages::STORAGE,
        });

        let material_bind_group_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            entries: &[BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Storage {
                        read_only: true
                    },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
            label: Some("material_bind_group_layout"),
        });

        let material_bind_group = device.create_bind_group(&BindGroupDescriptor {
            layout: &material_bind_group_layout,
            entries: &[BindGroupEntry {
                binding: 0,
                resource: material_buffer.as_entire_binding(),
            }],
            label: Some("material_bind_group"),
        });

        let render_pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("Render Pipeline Layout"),
            bind_group_layouts: &[&material_bind_group_layout],
            push_constant_ranges: &[],
        });

        let render_pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("Render Pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[
                    Vec3::desc(),
                ],
                compilation_options: PipelineCompilationOptions::default(),
            },
            fragment: Some(FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(ColorTargetState {
                    format: config.format,
                    blend: Some(BlendState::ALPHA_BLENDING),
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

        let vertex_buffer = device.create_buffer_init(&BufferInitDescriptor {
            label: Some("Vector buffer"),
            contents: bytemuck::cast_slice(&vertices),
            usage: BufferUsages::VERTEX,
        });

        let num_vertices = vertices.len() as u32;

        let settings = Settings {
            bg_color: Color {
                r: 0.1,
                g: 0.2,
                b: 0.3,
                a: 1.0,
            },
        };

        Self {
            surface,
            device,
            queue,
            config,
            size,
            window,
            render_pipeline,
            vertex_buffer,
            material_bind_group,
            num_vertices,
            settings
        }
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
        render_pass.set_bind_group(0, &self.material_bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        render_pass.draw(0..self.num_vertices, 0..1);
        drop(render_pass);
        self.queue.submit(iter::once(encoder.finish()));
        output.present();

        Ok(())
    }
}

impl ApplicationHandler for RayTracer {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window = Arc::new(event_loop.create_window(Window::default_attributes()).unwrap());
        let (doc, buffers, _) = gltf::import(GLTF_PATH).unwrap();
        let mut vertices = vec![];
        let mut materials = vec![];
        for mesh in doc.meshes() {
            for primitive in mesh.primitives() {
                let reader = primitive.reader(|buffer| Some(&buffers[buffer.index()]));
                if let Some(positions) = reader.read_positions() {
                    let corners: Vec<[f32; 3]> = positions.collect();
                    for vertex in corners {
                        vertices.push(vec3![vertex[0], vertex[1], vertex[2]]);
                    }
                }
            }
        }
        for material in doc.materials() {
            let material = material.pbr_metallic_roughness();
            let base_color = material.base_color_factor();
            materials.push(Material {
                ambient: [base_color[0], base_color[1], base_color[2], base_color[3]],
                diffuse: [0.0, 0.0, 0.0, 0.0],
                specular: [0.0, 0.0, 0.0, 0.0],
            });
        }
        self.state = Some(State::new(window, vertices, materials));
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
