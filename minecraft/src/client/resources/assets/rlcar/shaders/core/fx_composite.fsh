#version 330
#extension GL_ARB_separate_shader_objects : require

// The car effects over the scene, in linear light (dev.rlcar.client.LinearFx): SceneSampler is a
// copy of the main target (sRGB values), FxSampler the effects as linear premultiplied light (rgb)
// and coverage (alpha). The game adds and blends its particles in linear light this way:
// out = srgb(linear(scene) * (1 - fx.a) + fx.rgb), clamped to the display range at the end.

uniform sampler2D SceneSampler;
uniform sampler2D FxSampler;

layout(location = 0) in vec2 texCoord;

layout(location = 0) out vec4 fragColor;

vec3 srgbToLinear(vec3 c) {
    return mix(c / 12.92, pow((c + 0.055) / 1.055, vec3(2.4)), step(vec3(0.04045), c));
}

vec3 linearToSrgb(vec3 c) {
    c = clamp(c, 0.0, 1.0);
    return mix(c * 12.92, 1.055 * pow(c, vec3(1.0 / 2.4)) - 0.055, step(vec3(0.0031308), c));
}

void main() {
    ivec2 p = ivec2(gl_FragCoord.xy);
    vec4 fx = texelFetch(FxSampler, p, 0);
    vec4 scene = texelFetch(SceneSampler, p, 0);
    if (fx.a <= 0.0 && max(fx.r, max(fx.g, fx.b)) <= 0.0) {
        // No effect here: keep the pixel exactly as it is.
        fragColor = scene;
        return;
    }
    vec3 c = srgbToLinear(scene.rgb) * (1.0 - clamp(fx.a, 0.0, 1.0)) + max(fx.rgb, vec3(0.0));
    fragColor = vec4(linearToSrgb(c), scene.a);
}
