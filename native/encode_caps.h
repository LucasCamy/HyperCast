#pragma once
#include <cstdint>
// Fixed scalar ABI: 0 unknown, 1 yes, 2 no. No driver-owned pointers escape.
struct HcEncodeCaps {
    uint32_t support=0,min_width=0,min_height=0,max_width=0,max_height=0;
    uint32_t dynamic_bitrate=0,align_height=0,max_fps=0;
};
static_assert(sizeof(HcEncodeCaps)==32);
