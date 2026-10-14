"""Collect exact Ubuntu source archives for the Linux media/runtime bundle."""
import hashlib,json,re,subprocess,tarfile,tempfile,tomllib
from pathlib import Path
ROOT=Path(__file__).resolve().parent.parent

def main():
    records=json.loads((ROOT/".tools/linux-source-packages.json").read_text())
    version=tomllib.loads((ROOT/"Cargo.toml").read_text())["package"]["version"]
    if not re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+",version):raise ValueError("Invalid version")
    with tempfile.TemporaryDirectory(prefix="hypercast-source-") as folder:
        stage=Path(folder)
        for package,source_version in records:
            if not re.fullmatch(r"[a-z0-9][a-z0-9.+-]*",package) or not re.fullmatch(r"[A-Za-z0-9.+:~\-]+",source_version):raise ValueError("Invalid package record")
            subprocess.run(["apt-get","source","--download-only","-qq",f"{package}={source_version}"],cwd=stage,check=True)
        (stage/"packages.json").write_text(json.dumps(records,indent=2))
        (stage/"README.txt").write_text("Corresponding source packages for libraries and plugins bundled in the HyperCast Linux AppImage. Source versions match packages.txt inside the AppImage. Archives are downloaded without building or executing their contents. apt sources must include the matching Ubuntu deb-src repositories. HyperCast source and build scripts are available at https://github.com/LucasCamy/HyperCast.\n")
        entries=[p for p in sorted(stage.iterdir()) if p.is_file()]
        (stage/"SHA256SUMS").write_text("".join(f"{hashlib.sha256(p.read_bytes()).hexdigest()}  {p.name}\n" for p in entries))
        destination=ROOT/"dist"/f"HyperCast-{version}-linux-corresponding-source.tar.gz"
        with tarfile.open(destination,"w:gz") as archive:
            for p in sorted(stage.iterdir()):
                if p.is_file():archive.add(p,arcname=p.name)
        with (ROOT/"dist/SHA256SUMS-linux.txt").open("a") as output:output.write(f"{hashlib.sha256(destination.read_bytes()).hexdigest()}  {destination.name}\n")
        print(f"Collected {len(records)} exact source packages")
if __name__=="__main__":main()
