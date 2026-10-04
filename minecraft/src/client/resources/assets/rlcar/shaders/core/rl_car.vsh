#version 330
#extension GL_ARB_separate_shader_objects : require

// Vertex shader of the ports of Rocket League's car materials (see rlcar:rl_env.glsl). Entity
// vertex format, with the frame's lighting environment packed into the free attributes:
// Color.rgb = direction to the sun or moon * 0.5 + 0.5, Color.a = its strength,
// UV1 = sky colour (R | G << 8, B | moon << 8), UV2 = lightmap coordinates.

#include <minecraft:fog.glsl>
#include <minecraft:dynamictransforms.glsl>
#include <minecraft:projection.glsl>
#include <minecraft:sample_lightmap.glsl>

layout(location = 0) in vec3 Position;
layout(location = 1) in vec4 Color;
layout(location = 2) in vec2 UV0;
layout(location = 3) in ivec2 UV1;
layout(location = 4) in ivec2 UV2;
layout(location = 5) in vec3 Normal;

uniform sampler2D Sampler2;

layout(location = 0) out float sphericalVertexDistance;
layout(location = 1) out float cylindricalVertexDistance;
layout(location = 2) out vec3 worldPos;
layout(location = 3) out vec3 worldNormal;
layout(location = 4) out vec2 texCoord0;
layout(location = 5) out vec4 lightDirStrength;
layout(location = 6) out vec4 skyColorMoon;
layout(location = 7) out vec4 lightMapColor;
layout(location = 8) out float skyLevel;
#ifdef RL_BALL
// The ball's position along the field, -1..1 (rl_ball.fsh), in bits 8..14 of UV2.x.
layout(location = 9) out float fieldY;
#endif
#ifdef RL_WHEEL
// The wheel mesh's vertex colour red (1 = tyre, 0 = rim; rl_wheel.fsh) in bit 9 of UV2.x, bit 8
// set when the mesh has vertex colours; -1 without them.
layout(location = 9) out float vertexTyre;
#endif

void main() {
    gl_Position = ProjMat * ModelViewMat * vec4(Position, 1.0);

    sphericalVertexDistance = fog_spherical_distance(Position);
    cylindricalVertexDistance = fog_cylindrical_distance(Position);

    // Entity positions and normals are camera-relative world space.
    worldPos = Position;
    worldNormal = Normal;
    texCoord0 = UV0;
    lightDirStrength = vec4(Color.rgb * 2.0 - 1.0, Color.a);
    int a = UV1.x & 0xFFFF;
    int b = UV1.y & 0xFFFF;
    skyColorMoon = vec4(float(a & 255), float(a >> 8), float(b & 255), float(b >> 8)) / 255.0;
#ifdef RL_BALL
    ivec2 lightUv = ivec2(UV2.x & 0xFF, UV2.y);
    fieldY = float((UV2.x >> 8) & 0x7F) / 63.0 - 1.0;
#elif defined(RL_WHEEL)
    ivec2 lightUv = ivec2(UV2.x & 0xFF, UV2.y);
    vertexTyre = (UV2.x & 0x100) != 0 ? float((UV2.x >> 9) & 1) : -1.0;
#else
    ivec2 lightUv = UV2;
#endif
    lightMapColor = sample_lightmap(Sampler2, lightUv);
    skyLevel = clamp(float(lightUv.y) / 240.0, 0.0, 1.0);
}
