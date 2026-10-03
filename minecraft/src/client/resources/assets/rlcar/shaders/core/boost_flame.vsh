#version 330
#extension GL_ARB_separate_shader_objects : require

// Rocket League's boost flame cones (see boost_flame.fsh). Entity vertex format: UV0 is the mesh's
// first UV set, UV1.x its second one's U times 1000 (it marks the inner shell of the cone).

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
layout(location = 3) out float texCoord1U;
layout(location = 4) out vec3 normal;
layout(location = 5) out vec3 toCamera;

void main() {
    gl_Position = ProjMat * ModelViewMat * vec4(Position, 1.0);

    sphericalVertexDistance = fog_spherical_distance(Position);
    cylindricalVertexDistance = fog_cylindrical_distance(Position);

    texCoord0 = UV0;
    texCoord1U = float(UV1.x) / 1000.0;
    // Entity positions and normals are camera-relative world space.
    normal = Normal;
    toCamera = -Position;
}
