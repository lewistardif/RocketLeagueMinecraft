#version 330
#extension GL_ARB_separate_shader_objects : require

// Rocket League's boost smoke puffs: material SmokePuff_Mat, translated from the game's compiled
// pixel shader. Same as the Bevy demo's crates/rl_car_bevy/src/shaders/boost_smoke.wgsl.
// Translucent, unlit. UV is the particle's cell of the 4x4 sub-image grid; the material tiles its
// textures from it. Sampler0: Smoke01_D, Sampler1: Sphere_Gradient01_D,
// Sampler2: Radial_Generic_Tiling_Pack, all sRGB.

#include <minecraft:globals.glsl>
#include <minecraft:fog.glsl>
#include <minecraft:dynamictransforms.glsl>
#include <minecraft:oit.glsl>

uniform sampler2D Sampler0;
uniform sampler2D Sampler1;
uniform sampler2D Sampler2;

layout(location = 0) in float sphericalVertexDistance;
layout(location = 1) in float cylindricalVertexDistance;
layout(location = 2) in vec2 texCoord0;
layout(location = 3) in vec4 vertexColor;

#ifndef OIT_ALPHA_ONLY
layout(location = 0) out vec4 fragColor;
#endif

float sampleLinear(sampler2D s, vec2 uv) {
    float c = texture(s, uv).x;
    return c <= 0.04045 ? c / 12.92 : pow((c + 0.055) / 1.055, 2.4);
}

float linearToSrgb(float c) {
    return c <= 0.0031308 ? c * 12.92 : 1.055 * pow(c, 1.0 / 2.4) - 0.055;
}

void main() {
    vec2 uv = texCoord0;
    float time = GameTime * 1200.0; // seconds
    vec2 panA = fract(vec2(time * 0.038, time * 0.05));
    vec2 panB = fract(vec2(time * -0.00175, time * -0.05));

    float a = sampleLinear(Sampler0, uv * 3.0 + panA);
    float b = sampleLinear(Sampler0, uv * 1.53 + panB);
    float gradient = sampleLinear(Sampler1, uv * 4.0);
    float radial = sampleLinear(Sampler2, uv * 4.0);

    float alpha = clamp(a * b * radial * 48.0 - 0.25, 0.0, 1.0);
    vec3 rgb = clamp((3.0 - alpha) * (a + b) * gradient * vertexColor.rgb, 0.0, 1.0);
    vec4 color = vec4(linearToSrgb(rgb.r), linearToSrgb(rgb.g), linearToSrgb(rgb.b), alpha * vertexColor.a) * ColorModulator;

    #ifdef OIT_ALPHA_ONLY
    executeAlphaOnlyPhase(gl_FragCoord.z, color.a);
    #else
    #ifdef OIT_ACCUMULATE
    color = sampleColorForAccumulation(color);
    vec4 fogColor = vec4(FogColor.rgb * color.a, FogColor.a);
    #else
    vec4 fogColor = FogColor;
    #endif
    fragColor = apply_fog(color, sphericalVertexDistance, cylindricalVertexDistance, FogEnvironmentalStart, FogEnvironmentalEnd, FogRenderDistanceStart, FogRenderDistanceEnd, fogColor);
    #endif
}
