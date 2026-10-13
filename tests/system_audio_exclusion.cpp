// Synthetic playout on the same process must not enter its loopback capture.
#define NOMINMAX
#include <windows.h>
#include <array>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <memory>
#include <stdexcept>
#include <vector>
extern "C" {
void* hc_audio_capture_open(char*,size_t) noexcept;
int hc_audio_capture_read(void*,int16_t*,uint32_t,char*,size_t) noexcept;
void hc_audio_capture_close(void*) noexcept;
void* hc_audio_playout_open(char*,size_t) noexcept;
int hc_audio_playout_write(void*,const int16_t*,uint32_t,char*,size_t) noexcept;
void hc_audio_playout_close(void*) noexcept;
}
int main() {
    try {
        std::array<char,1024> error{};
        std::unique_ptr<void,decltype(&hc_audio_capture_close)> capture(hc_audio_capture_open(error.data(),error.size()),hc_audio_capture_close);
        if (!capture) throw std::runtime_error(error.data());
        std::unique_ptr<void,decltype(&hc_audio_playout_close)> output(hc_audio_playout_open(error.data(),error.size()),hc_audio_playout_close);
        if (!output) throw std::runtime_error(error.data());
        std::array<int16_t,960> tone{}; std::vector<int16_t> input(96000);
        int peak = 0, writes = 0; uint64_t position = 0;
        auto until = GetTickCount64()+1500;
        while (GetTickCount64()<until) {
            for (size_t i=0; i<480; ++i) { auto value=static_cast<int16_t>(200*std::sin(6.28318530718*1373*(position+i)/48000.0)); tone[2*i]=tone[2*i+1]=value; }
            int written=hc_audio_playout_write(output.get(),tone.data(),480,error.data(),error.size());
            if (written<0) throw std::runtime_error(error.data());
            if (written) { position+=480; ++writes; }
            int frames=hc_audio_capture_read(capture.get(),input.data(),static_cast<uint32_t>(input.size()),error.data(),error.size());
            if (frames<0) throw std::runtime_error(error.data());
            for (int i=0; i<frames*2; ++i) peak=std::max(peak,std::abs(static_cast<int>(input[i])));
        }
        if (writes<40) throw std::runtime_error("Insufficient WASAPI output");
        if (peak>2) throw std::runtime_error("Loopback saw external audio; repeat exclusion test with other apps silent");
        if (hc_audio_capture_read(capture.get(),input.data(),96001,error.data(),error.size())!=-1) throw std::runtime_error("Capture bound bypassed");
        if (hc_audio_playout_write(output.get(),tone.data(),961,error.data(),error.size())!=-1) throw std::runtime_error("Output bound bypassed");
        std::puts("PASS: system loopback excludes own process playout; native PCM bounds enforced");
        return 0;
    } catch (const std::exception& error) { std::fprintf(stderr,"%s\n",error.what()); return 1; }
}
