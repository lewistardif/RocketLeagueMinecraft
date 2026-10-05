#version 330
#extension GL_ARB_separate_shader_objects : require

// Rocket League's boost meter, one layer of its Scaleform clip (see CarHud.java). Entity vertex
// format: Position = GUI x, y and the perspective divisor w; Color = the colour transform's
// multipliers; UV1, UV2.x = its colour offsets (-255..255); UV2.y = alpha offset + 255 + 512 * the
// alpha channel selector (0 texture alpha, 1 R glyph, 2 G glow 6, 3 B glow 8).

#include <minecraft:dynamictransforms.glsl>
#include <minecraft:projection.glsl>

layout(location = 0) in vec3 Position;
layout(location = 1) in vec4 Color;
layout(location = 2) in vec2 UV0;
layout(location = 3) in ivec2 UV1;
layout(location = 4) in ivec2 UV2;
layout(location = 5) in vec3 Normal;

layout(location = 0) out vec2 texCoord0;
layout(location = 1) out vec4 colorMult;
layout(location = 2) out vec4 colorAdd;
layout(location = 3) flat out int channel;

void main() {
    // Scaling the whole clip position by w keeps the point and makes interpolation perspective-correct.
    gl_Position = ProjMat * ModelViewMat * vec4(Position.xy, 0.0, 1.0) * Position.z;
    texCoord0 = UV0;
    colorMult = Color;
    channel = UV2.y / 512;
    colorAdd = vec4(float(UV1.x), float(UV1.y), float(UV2.x), float(UV2.y - channel * 512 - 255)) / 255.0;
}
