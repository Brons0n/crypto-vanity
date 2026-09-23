// common.h is prepended by the host before runtime compilation as Metal Shading Language.
kernel void search(device uint *points [[buffer(0)]], device const uint *params [[buffer(1)]],
                   device uint *hits [[buffer(2)]], uint lane [[thread_position_in_grid]]) {
    search_lane(lane, points, params, hits);
}
