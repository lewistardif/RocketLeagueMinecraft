#version 330
#extension GL_ARB_separate_shader_objects : require

// The ball's markers (see rl_marker.fsh), built on the CPU around the ball. Entity vertex format:
// Color is the marker's colour and alpha (the ground reticle puts its altitude in red), UV0 the
// texture coordinate, Normal the clarity sphere's (inward) normal.

#include <minecraft:fog.glsl>
#include <minecraft:dynamictransforms.glsl>
#include <minecraft:projection.glsl>

layout(location = 0) in vec3 Position;
layout(location = 1) in vec4 Color;
layout(location = 2) in vec2 UV0;
layout(location = 3) in ivec2 UV1;
layout(location = 4) in ivec2 UV2;
layout(location = 5) in vec3 Normal;

layout(location = 0) out float sphericalVertexDistance;
layout(location = 1) out float cylindricalVertexDistance;
layout(location = 2) out vec2 texCoord0;
layout(location = 3) out vec4 vertexColor;
layout(location = 4) out vec3 worldPos;
layout(location = 5) out vec3 worldNormal;
layout(location = 6) out float viewDepth;

void main() {
    vec4 view = ModelViewMat * vec4(Position, 1.0);
    gl_Position = ProjMat * view;

    sphericalVertexDistance = fog_spherical_distance(Position);
    cylindricalVertexDistance = fog_cylindrical_distance(Position);

    texCoord0 = UV0;
    vertexColor = Color;
    // Entity positions and normals are camera-relative world space.
    worldPos = Position;
    worldNormal = Normal;
    viewDepth = -view.z;
}
