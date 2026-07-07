#!/usr/bin/env python3
"""Build and publish the AgentOS Python SDK to PyPI.

Usage:
    python scripts/publish_pypi.py [--test] [--build-only]

Options:
    --test         Upload to TestPyPI instead of production PyPI
    --build-only   Only build the wheel, don't upload
"""

import argparse
import os
import shutil
import subprocess
import sys
import tempfile


def run(cmd: list[str], cwd: str | None = None) -> None:
    print(f"  $ {' '.join(cmd)}")
    subprocess.check_call(cmd, cwd=cwd)


def build_sdk(source_dir: str, output_dir: str) -> str:
    """Build the Python SDK wheel. Returns path to the wheel."""
    sdk_dir = os.path.join(source_dir, "crates", "sdk", "python")

    if not os.path.exists(sdk_dir):
        print(f"✗ Python SDK directory not found: {sdk_dir}")
        print("  Expected at crates/sdk/python/")
        sys.exit(1)

    # Install build dependencies
    print("\n→ Installing build dependencies...")
    run([sys.executable, "-m", "pip", "install", "--quiet", "build", "twine"], cwd=sdk_dir)

    # Clean previous builds
    for d in ["build", "dist", "*.egg-info"]:
        shutil.rmtree(os.path.join(sdk_dir, d), ignore_errors=True)

    # Build the wheel
    print("\n→ Building wheel...")
    run([sys.executable, "-m", "build", "--wheel"], cwd=sdk_dir)

    # Find the built wheel
    dist_dir = os.path.join(sdk_dir, "dist")
    wheels = [f for f in os.listdir(dist_dir) if f.endswith(".whl")]
    if not wheels:
        print("✗ No wheel found in dist/")
        sys.exit(1)

    wheel_path = os.path.join(dist_dir, wheels[0])
    dest = os.path.join(output_dir, wheels[0])
    shutil.copy2(wheel_path, dest)

    print(f"\n✓ Built: {dest}")
    return dest


def publish(wheel_path: str, test: bool = False) -> None:
    """Upload the wheel to PyPI."""
    repo_url = "--repository-url https://test.pypi.org/legacy/" if test else ""
    
    print(f"\n→ Uploading to {'TestPyPI' if test else 'PyPI'}...")
    
    cmd = [sys.executable, "-m", "twine", "upload"]
    if test:
        cmd.extend(["--repository-url", "https://test.pypi.org/legacy/"])
    cmd.append(wheel_path)
    
    try:
        run(cmd)
        print(f"\n✓ Published to {'TestPyPI' if test else 'PyPI'}!")
    except subprocess.CalledProcessError as e:
        print(f"\n✗ Upload failed: {e}")
        print("  Make sure you have:")
        print("    1. Registered on PyPI: https://pypi.org/account/register/")
        print("    2. Created an API token: https://pypi.org/manage/account/token/")
        print("    3. Set it in ~/.pypirc or via TWINE_USERNAME/TWINE_PASSWORD")
        sys.exit(1)


def main():
    parser = argparse.ArgumentParser(description="Build and publish AgentOS Python SDK")
    parser.add_argument("--test", action="store_true", help="Upload to TestPyPI")
    parser.add_argument("--build-only", action="store_true", help="Only build, don't upload")
    args = parser.parse_args()

    repo_root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    output_dir = tempfile.mkdtemp(prefix="agentos-pypi-")

    print("╔════════════════════════════════════════════════════╗")
    print("║  AgentOS Python SDK — PyPI Publisher               ║")
    print("╚════════════════════════════════════════════════════╝")

    wheel = build_sdk(repo_root, output_dir)

    if not args.build_only:
        publish(wheel, test=args.test)
    else:
        print(f"\n✓ Wheel built (not uploaded): {wheel}")

    # Cleanup
    shutil.rmtree(output_dir, ignore_errors=True)
    print("\nDone.")


if __name__ == "__main__":
    main()
