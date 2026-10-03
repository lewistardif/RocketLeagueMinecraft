// Rocket League's boost smoke puffs: material `SmokePuff_Mat`, translated from the game's compiled
// pixel shader (RefShaderCache, sub-UV particle sprites). Translucent, unlit.
//
// UV is the particle's cell of the 4x4 sub-image grid, as the game's sub-UV sprites get it; the
// shader tiles its textures from it, so the cell only offsets the noise. The vertex colour is the
// particle colour (HDR) and alpha. Textures: 0 `Smoke01_D`, 1 `Sphere_Gradient01_D`,
// 2 `Radial_Generic_Tiling_Pack`.

#import bevy_pbr::forward_io::VertexOutput
#import bevy_pbr::mesh_view_bindings::globals

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var smoke_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var smoke_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var gradient_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var gradient_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(4) var radial_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(5) var radial_sampler: sampler;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let uv = in.uv;
#ifdef VERTEX_COLORS
    let color = in.color;
#else
    let color = vec4(1.0);
#endif
    let time = globals.time;
    // The material's two panners (frac(Time * speed) per axis).
    let pan_a = fract(vec2(time * 0.038, time * 0.05));
    let pan_b = fract(vec2(time * -0.00175, time * -0.05));

    let a = textureSample(smoke_texture, smoke_sampler, uv * 3.0 + pan_a).x;
    let b = textureSample(smoke_texture, smoke_sampler, uv * 1.53 + pan_b).x;
    let gradient = textureSample(gradient_texture, gradient_sampler, uv * 4.0).x;
    let radial = textureSample(radial_texture, radial_sampler, uv * 4.0).x;

    let alpha = saturate(a * b * radial * 48.0 - 0.25);
    let rgb = (3.0 - alpha) * (a + b) * gradient * color.rgb;
    return vec4(rgb, alpha * color.a);
}
