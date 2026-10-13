`nvEncodeAPI.h` comes from FFmpeg/nv-codec-headers commit
`eddcea9e27f6b772057c9b3f87de2cc1737faffc`, Video Codec SDK 13.1.15.
The NVIDIA permissive license is preserved at the top of the header.
This API version requires NVIDIA driver 610 or newer.
No NVIDIA SDK binary is shipped: the bridge loads the driver-provided
`nvEncodeAPI64.dll` exclusively from Windows System32.
