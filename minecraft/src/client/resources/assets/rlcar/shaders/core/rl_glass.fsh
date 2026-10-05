#version 330
#extension GL_ARB_separate_shader_objects : require

// Separate glass parts (some bodies' windshields and headlight lenses): dark tinted glass that
// reflects the sky with a Schlick Fresnel term and a sharp sun highlight, in the same light as
// the ported car materials (rlcar:rl_env.glsl). Not a port: these materials are not in the cars
// the mod extracts often enough to warrant one.

#include <rlcar:rl_env.glsl>

void main() {
    RlEnv e = rlEnv();
    float nv = clamp(dot(e.N, e.V), 0.0, 1.0);
    float fresnel = 0.04 + 0.96 * pow(1.0 - nv, 5.0);
    vec3 r = reflect(-e.V, e.N);
    vec3 h = normalize(e.L + e.V);
    vec3 color = vec3(0.012, 0.014, 0.018) * (shIrradiance(e, e.N) + skyUpper(e));
    color += envCube(e, r) * fresnel;
    color += e.lightColor * pow(clamp(dot(e.N, h), 0.0, 1.0), 300.0) * 2.0;
    fragColor = rlFinish(color);
}
