#version 330
#extension GL_ARB_separate_shader_objects : require

// Rocket League's boost meter (see boost_meter.vsh): Flash's colour transform,
// c' = clamp(c * mult + add), on the texture (bitmaps) or on black with the alpha from a glyph
// atlas channel (text and its glow). Straight alpha, gamma space, like Scaleform.

#include <minecraft:dynamictransforms.glsl>

uniform sampler2D Sampler0;

layout(location = 0) in vec2 texCoord0;
layout(location = 1) in vec4 colorMult;
layout(location = 2) in vec4 colorAdd;
layout(location = 3) flat in int channel;

layout(location = 0) out vec4 fragColor;

void main() {
    vec4 s = texture(Sampler0, texCoord0);
    vec3 rgb = channel == 0 ? s.rgb : vec3(0.0);
    float a = channel == 0 ? s.a : channel == 1 ? s.r : channel == 2 ? s.g : s.b;
    vec4 color = clamp(vec4(rgb, a) * colorMult + colorAdd, 0.0, 1.0);
    if (color.a == 0.0) {
        discard;
    }
    fragColor = color * ColorModulator;
}
