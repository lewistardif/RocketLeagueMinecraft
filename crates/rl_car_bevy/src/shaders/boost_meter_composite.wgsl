// Composites the boost meter's own buffer (premultiplied, gamma-space colour; see `hud.rs`) over
// the scene: un-premultiply, convert to linear light, blend.

#import bevy_ui::ui_vertex_output::UiVertexOutput

@group(1) @binding(0) var layer_texture: texture_2d<f32>;
@group(1) @binding(1) var layer_sampler: sampler;

fn srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    return select(pow((c + 0.055) / 1.055, vec3(2.4)), c / 12.92, c <= vec3(0.04045));
}

@fragment
fn fragment(in: UiVertexOutput) -> @location(0) vec4<f32> {
    let s = textureSample(layer_texture, layer_sampler, in.uv);
    if s.a <= 0.0 {
        return vec4(0.0);
    }
    let rgb = clamp(s.rgb / s.a, vec3(0.0), vec3(1.0));
    return vec4(srgb_to_linear(rgb), min(s.a, 1.0));
}
