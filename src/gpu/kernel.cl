// common.h is prepended by the host before runtime compilation.
__kernel void search(__global uint *points, __global const uint *params, __global uint *hits) {
    search_lane((uint)get_global_id(0), points, params, hits);
}
