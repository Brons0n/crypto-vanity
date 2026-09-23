"""Package only the native CPU binary, launchers, and public documentation.

Run on each native CI runner after tests. This script makes no network requests.
"""
import argparse
import os
from pathlib import Path
import platform
import shutil
import subprocess
import tarfile
import tempfile
import zipfile


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--label", required=True, choices=[
        "windows-x64", "linux-x64", "macos-arm64", "macos-x64",
    ])
    args = parser.parse_args()
    expected = {
        "windows-x64": ("Windows", {"AMD64", "x86_64"}),
        "linux-x64": ("Linux", {"x86_64"}),
        "macos-arm64": ("Darwin", {"arm64", "aarch64"}),
        "macos-x64": ("Darwin", {"x86_64"}),
    }[args.label]
    if platform.system() != expected[0] or platform.machine() not in expected[1]:
        parser.error("label must match the native build machine")
    root = Path(__file__).resolve().parents[1]
    windows = platform.system() == "Windows"
    binary = "vanitybtc.exe" if windows else "vanitybtc"
    files = ["README.md", "LICENSE"]
    files += ["start-vanity.bat"] if windows else ["start-vanity.sh"]
    if platform.system() == "Darwin":
        files.append("Start Vanity.command")
    name = "vanity-generator-" + args.label
    output = root / "dist"
    output.mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory() as temporary:
        folder = Path(temporary) / name
        folder.mkdir()
        shutil.copy2(root / "target" / "release" / binary, folder / binary)
        for source in files:
            shutil.copy2(root / source, folder / source)
        if not windows:
            for executable in [binary, "start-vanity.sh", "Start Vanity.command"]:
                path = folder / executable
                if path.exists():
                    path.chmod(0o755)
        # Exercise the distributed launcher, exiting before any key generation.
        # Only fixed assertions are printed; captured output is never logged.
        if windows:
            command = [os.environ.get("COMSPEC", "cmd.exe"), "/d", "/c", "start-vanity.bat"]
        else:
            command = [str(folder / ("Start Vanity.command" if platform.system() == "Darwin" else "start-vanity.sh"))]
        env = os.environ.copy()
        env["NO_COLOR"] = "1"
        env.pop("CLICOLOR_FORCE", None)
        result = subprocess.run(command, cwd=folder, input=b"0\n\n", capture_output=True, timeout=20, env=env)
        screen = result.stderr.decode("utf-8")
        if result.returncode != 0 or "VANITY GENERATOR" not in screen or "Networks [Enter: bitcoin]" not in screen:
            raise RuntimeError("packaged launcher failed its UI startup check")
        if b"Private key" in result.stdout or "Make an address with your own text" in screen:
            raise RuntimeError("packaged launcher has unexpected output")
        lines = screen.splitlines()
        top = next(i for i, line in enumerate(lines) if "╭" in line)
        box = lines[top:top + 3]
        if len(set(map(len, box))) != 1 or box[1].split("│")[1] != "VANITY GENERATOR".center(48):
            raise RuntimeError("packaged UI header is not aligned")
        if windows:
            archive = output / (name + ".zip")
            with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED) as bundle:
                for path in folder.iterdir():
                    bundle.write(path, arcname=f"{name}/{path.name}")
        else:
            archive = output / (name + ".tar.gz")
            with tarfile.open(archive, "w:gz") as bundle:
                bundle.add(folder, arcname=name)
    print(f"Packaged {archive.name}; native launcher and shared UI checks passed.")


if __name__ == "__main__":
    main()
