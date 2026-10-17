"""Build a Linux AppImage with a fixed media allowlist; never copy workspace secrets."""
import argparse,hashlib,json,os,re,shutil,subprocess,tempfile,tomllib,urllib.request
from pathlib import Path
ROOT=Path(__file__).resolve().parent.parent
TOOL_URL="https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage"
TOOL_SHA256="95cbe7cce9717fce90c484e34052ee7c7f1d7635b33c12525b4776826a7d29b6"
PLUGIN_NAMES=("coreelements","app","audioconvert","audioresample","videoconvertscale","videorate","videoparsersbad","nvcodec","va","x265","pulseaudio","pipewire","ximagesrc","videotestsrc","audiotestsrc","typefindfunctions")
HOST_LIBS=re.compile(r"^(ld-linux.*|lib(c|m|dl|pthread|rt|resolv|util|nss_.*)\.so\..*|lib(GL|EGL|GLX|OpenGL|GLdispatch|vulkan|drm|gbm|glapi).*|lib(nvidia|cuda|nvcuvid|nvidia-encode).*)$")
def run(*args,**kwargs):return subprocess.run(args,check=True,text=True,**kwargs)
def sha(path):return hashlib.sha256(path.read_bytes()).hexdigest()
def bundle_binary(source,libdir,seen,packages):
    source=Path(source)
    if source.name in seen:return
    seen.add(source.name)
    if not source.is_file():raise RuntimeError(f"Required library missing: {source}")
    shutil.copy2(source,libdir/source.name)
    ownership=subprocess.run(["dpkg-query","-S",str(source.resolve())],capture_output=True,text=True)
    if ownership.returncode==0:
        for line in ownership.stdout.splitlines():packages.add(line.split(": ",1)[0])
    output=run("ldd",str(source),capture_output=True).stdout
    if "not found" in output:raise RuntimeError(f"Unresolved library dependency: {source.name}")
    for line in output.splitlines():
        match=re.search(r"=> (/[^ ]+)",line)
        if match:
            dependency=Path(match.group(1))
            if not HOST_LIBS.match(dependency.name):bundle_binary(dependency,libdir,seen,packages)
def main():
    parser=argparse.ArgumentParser();parser.add_argument("--binary",type=Path,required=True);parser.add_argument("--media-prefix",type=Path);parser.add_argument("--output",type=Path,default=ROOT/"dist");args=parser.parse_args()
    if os.name!="posix":raise SystemExit("Run this packager on Linux")
    version=tomllib.loads((ROOT/"Cargo.toml").read_text())["package"]["version"]
    if not re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+(?:-[a-z0-9.]+)?",version):raise ValueError("Invalid release version")
    args.output.mkdir(parents=True,exist_ok=True)
    tools=ROOT/".tools/linux";tools.mkdir(parents=True,exist_ok=True);tool=tools/"appimagetool-x86_64.AppImage"
    if not tool.exists():
        with urllib.request.urlopen(TOOL_URL,timeout=60) as response,tool.open("wb") as output:shutil.copyfileobj(response,output)
    if sha(tool)!=TOOL_SHA256:raise RuntimeError("AppImage tool checksum mismatch; verify upstream asset before updating the pin")
    tool.chmod(0o755)
    with tempfile.TemporaryDirectory(prefix="hypercast-appimage-") as temp:
        stage=Path(temp)/"HyperCast.AppDir";libdir=stage/"usr/lib";bindir=stage/"usr/bin";plugins=libdir/"gstreamer-1.0";licenses=stage/"usr/share/hypercast/licenses"
        for folder in (libdir,bindir,plugins,licenses):folder.mkdir(parents=True,exist_ok=True)
        shutil.copy2(args.binary,bindir/"hypercast");(bindir/"hypercast").chmod(0o755)
        seen=set();packages=set();pluginroot=Path("/usr/lib/x86_64-linux-gnu/gstreamer-1.0")
        media=args.media_prefix.resolve() if args.media_prefix else None
        if media:
            os.environ["LD_LIBRARY_PATH"]=str(media/"lib")
            os.environ["PKG_CONFIG_PATH"]=str(media/"lib/pkgconfig")
        for name in PLUGIN_NAMES:
            source=pluginroot/f"libgst{name}.so"
            override=media/"lib/gstreamer-1.0"/source.name if media else None
            if override and override.is_file():source=override
            if not source.is_file():
                if name=="va":continue
                raise RuntimeError(f"Media plugin missing: {name}")
            # Keep the ELF at its plugin path, while dependencies live in usr/lib.
            bundle_binary(source,libdir,seen,packages);shutil.move(libdir/source.name,plugins/source.name)
        if media:
            (licenses/"GStreamer-build.txt").write_text("GStreamer core/base/bad 1.26.10 built from official archives. NVENC modern encoder with P1 and ultra-low-latency tuning. Build scripts and source archives accompany this release.\n")
        # dlopened FFmpeg and GStreamer libraries must be explicitly included.
        for pkg in ("libavcodec","libavutil","gstreamer-1.0"):
            directory=Path(run("pkg-config","--variable=libdir",pkg,capture_output=True).stdout.strip())
            prefix={"libavcodec":"libavcodec","libavutil":"libavutil","libswscale":"libswscale","gstreamer-1.0":"libgstreamer-1.0"}[pkg]
            source=sorted(directory.glob(prefix+".so.[0-9]*"),key=lambda p:len(p.name))[0];bundle_binary(source,libdir,seen,packages)
        # Include all direct binary dependencies too.
        output=run("ldd",str(args.binary),capture_output=True).stdout
        for path in re.findall(r"=> (/[^ ]+)",output):
            if not HOST_LIBS.match(Path(path).name):bundle_binary(path,libdir,seen,packages)
        base=pluginroot.parent
        for relative in ("spa-0.2/support","spa-0.2/audioconvert","spa-0.2/videoconvert","pipewire-0.3"):
            directory=base/relative
            if not directory.exists():continue
            destination=libdir/relative;destination.mkdir(parents=True,exist_ok=True)
            for source in directory.glob("*.so"):
                if relative=="pipewire-0.3" and source.name not in {"libpipewire-module-protocol-native.so","libpipewire-module-client-node.so","libpipewire-module-adapter.so","libpipewire-module-metadata.so"}:continue
                bundle_binary(source,libdir,seen,packages)
                if (libdir/source.name).exists():shutil.move(libdir/source.name,destination/source.name)
        pipewire=Path("/usr/share/pipewire")
        if pipewire.exists():shutil.copytree(pipewire,stage/"usr/share/pipewire")
        for package in sorted(packages):
            package_name=package.split(":",1)[0];copyright=Path("/usr/share/doc")/package_name/"copyright"
            if copyright.is_file():shutil.copy2(copyright,licenses/f"{package_name}-copyright.txt")
        (licenses/"packages.txt").write_text(run("dpkg-query","-W","-f=${binary:Package} ${Version} ${source:Package} ${source:Version}\n",*sorted(packages),capture_output=True).stdout)
        (ROOT/".tools/linux-runtime-packages.json").write_text(json.dumps(sorted(packages)))
        package_records=run("dpkg-query","-W","-f=${source:Package}\t${source:Version}\n",*sorted(packages),capture_output=True).stdout
        source_records=sorted(set(tuple(line.split("\t",1)) for line in package_records.splitlines() if "\t" in line))
        (ROOT/".tools/linux-source-packages.json").write_text(json.dumps(source_records))
        for notice in ("GPL-2","GPL-3","LGPL-2.1","LGPL-3","Apache-2.0","BSD","MPL-2.0"):
            source=Path("/usr/share/common-licenses")/notice
            if source.is_file():shutil.copy2(source,licenses/(notice+".txt"))
        for name in ("LICENSE","docs/THIRD_PARTY.md","docs/linux.md"):
            source=ROOT/name
            if source.exists():shutil.copy2(source,licenses/source.name)
        (stage/"com.cloudlane.HyperCast.desktop").write_text("[Desktop Entry]\nType=Application\nName=HyperCast\nComment=Screen sharing through LiveKit\nExec=hypercast\nIcon=hypercast\nCategories=Network;AudioVideo;\nTerminal=false\n")
        shutil.copy2(ROOT/"assets/hypercast.png",stage/"hypercast.png")
        shutil.copy2(ROOT/"tools/linux/AppRun",stage/"AppRun")
        (stage/"AppRun").chmod(0o755)
        scanner=Path(run("pkg-config","--variable=pluginscannerdir","gstreamer-1.0",capture_output=True).stdout.strip())/"gst-plugin-scanner"
        if not scanner.exists():scanner=Path("/usr/lib/x86_64-linux-gnu/gstreamer1.0/gstreamer-1.0/gst-plugin-scanner")
        shutil.copy2(scanner,libdir/"gst-plugin-scanner")
        metadata=json.loads(run("cargo","metadata","--locked","--features","desktop","--filter-platform","x86_64-unknown-linux-gnu","--format-version","1",cwd=ROOT,capture_output=True).stdout)
        with (licenses/"Rust-dependencies.txt").open("w") as output:
            for package in sorted(metadata["packages"],key=lambda p:(p["name"],p["version"])):
                output.write(f"\n{package['name']} {package['version']} · {package.get('license')} · {package.get('repository')}\n")
                root=Path(package["manifest_path"]).parent
                for source in sorted(set(root.glob("LICENSE*"))|set(root.glob("COPYING*"))|set(root.glob("NOTICE*"))):
                    if source.is_file() and source.stat().st_size<2*1024*1024:output.write(source.read_text(errors="replace")+"\n")
        cargo_target=Path(os.environ.get("CARGO_TARGET_DIR",str(ROOT/"target")))
        notices=list(cargo_target.glob("*/build/scratch-*/out/livekit_webrtc/**/LICENSE.md"))
        if not notices:raise RuntimeError("WebRTC third-party notice missing from build output")
        shutil.copy2(notices[0],licenses/"WebRTC-LICENSE.md")
        fonts=[p for p in metadata["packages"] if p["name"]=="epaint_default_fonts"]
        for font in fonts:
            directory=Path(font["manifest_path"]).parent/"fonts"
            for notice in directory.glob("*.txt"):shutil.copy2(notice,licenses/("fonts-"+notice.name))
        destination=args.output/f"HyperCast-{version}-linux-x86_64.AppImage"
        run(str(tool),"--appimage-extract-and-run",str(stage),str(destination),env={**os.environ,"ARCH":"x86_64"})
        destination.chmod(0o755)
        (args.output/"SHA256SUMS-linux.txt").write_text(f"{sha(destination)}  {destination.name}\n")
        print(f"Packaged {destination.name}")
if __name__=="__main__":main()
