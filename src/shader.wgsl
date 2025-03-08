struct VertexInput {
    @location(0) position: vec3f,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4f,
};

struct Material {
    ambient: vec4f,
    diffuse: vec4f,
    specular: vec4f,
};

@group(0) @binding(0) var<storage, read> materials: array<Material>;

@vertex
fn vs_main(
    model: VertexInput,
) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = vec4f(model.position, 1.0);
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4f {
    return materials[0].ambient;
}
