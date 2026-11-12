"""Resolve the candidate version and the older stable release used for upgrade tests."""

import json
import os
from pathlib import Path
import re
import subprocess
import tomllib


def version_tuple(value):
    if not re.fullmatch(r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)", value):
        raise ValueError(f"Expected a stable version, got {value}")
    return tuple(map(int, value.split(".")))


version = tomllib.loads(Path("Cargo.toml").read_text(encoding="utf-8"))["package"]["version"]
candidate = version_tuple(version)
repository = os.environ["GITHUB_REPOSITORY"]
pages = json.loads(subprocess.check_output(
    ["gh", "api", "--paginate", "--slurp", f"repos/{repository}/releases?per_page=100"],
    text=True,
))
stable = {}
for page in pages:
    for release in page:
        tag = release["tag_name"]
        if release["draft"] or release["prerelease"] or not tag.startswith("v"):
            continue
        try:
            stable[version_tuple(tag[1:])] = tag
        except ValueError:
            continue
if os.environ.get("RELEASE_PUBLISH") == "true" and stable and candidate <= max(stable):
    raise SystemExit(f"Candidate v{version} must be newer than {stable[max(stable)]}")
older = [previous for previous in stable if previous < candidate]
if not older:
    raise SystemExit(f"No older stable release for the v{version} upgrade test")
baseline = stable[max(older)]
with Path(os.environ["GITHUB_OUTPUT"]).open("a", encoding="utf-8") as output:
    output.write(f"version={version}\nbaseline={baseline}\n")
print(f"Candidate v{version}, upgrade baseline {baseline}")
