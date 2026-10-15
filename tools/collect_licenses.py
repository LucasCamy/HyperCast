"""Collect shipped crate notices from locked Cargo metadata, including font licenses."""
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

def main():
    metadata=json.loads((ROOT/".tools/package-metadata.json").read_text(encoding="utf-8-sig"))
    destination=ROOT/"dist/HyperCast-0.8.3-windows-x64/licenses/Rust-dependencies.txt"
    destination.parent.mkdir(parents=True,exist_ok=True)
    with destination.open("w",encoding="utf-8") as output:
        for package in sorted(metadata["packages"],key=lambda p:(p["name"],p["version"])):
            output.write(f"\n{'='*72}\n{package['name']} {package['version']}\nDeclared license: {package.get('license')}\nRepository: {package.get('repository')}\n")
            root=Path(package["manifest_path"]).parent
            paths=set()
            for pattern in ("LICENSE*","COPYING*","NOTICE*","*license*","*Licence*"):
                paths.update(p for p in root.glob(pattern) if p.is_file())
            if package["name"] == "epaint_default_fonts":
                paths.update(p for p in (root/"fonts").iterdir() if p.suffix.lower() in {".txt",".md"})
            for path in sorted(paths):
                if path.stat().st_size > 2*1024*1024:
                    raise ValueError("Unexpectedly large license notice")
                output.write(f"\n--- {path.relative_to(root)} ---\n")
                output.write(path.read_text(encoding="utf-8",errors="replace"))
                output.write("\n")
    print("Locked Rust and font dependency notices collected")

if __name__ == "__main__":
    main()
