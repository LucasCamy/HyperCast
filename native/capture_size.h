#pragma once
#include <algorithm>
#include <cmath>
#include <cstdint>
#include <stdexcept>
static constexpr uint64_t hc_max_pixels = 33554432;
inline bool hc_capture_size(uint32_t w,uint32_t h) {
    return w>=2 && h>=2 && w<=8192 && h<=8192 && uint64_t(w)*h<=hc_max_pixels;
}
inline void hc_fit_size(uint32_t& w,uint32_t& h,uint32_t max_w,uint32_t max_h) {
    if(!hc_capture_size(w,h) || max_w<2 || max_h<2) throw std::runtime_error("Invalid source/encoder resolution (maximum 8192 per axis and 32 megapixels)");
    max_w=std::min(max_w,8192u)&~1u;max_h=std::min(max_h,8192u)&~1u;
    const double scale=std::min({1.0,double(max_w)/w,double(max_h)/h});
    w=std::clamp(uint32_t(std::round(w*scale/2)*2),2u,max_w);
    h=std::clamp(uint32_t(std::round(h*scale/2)*2),2u,max_h);
    while(uint64_t(w)*h>hc_max_pixels) {if(w>=h)w-=2;else h-=2;}
}
