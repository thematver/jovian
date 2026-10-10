#!/usr/bin/env python3
"""Exercise build/install failure paths with fake tools and temporary bundles only."""

import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


SCRIPT = Path(__file__).resolve().parents[1] / "build.sh"
BUNDLES = ("CLAP/Jovian-8.clap", "VST3/Jovian-8.vst3", "Components/Jovian-8.component")


class BuildScriptTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="jovian-build-test-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        (self.root / "Tools").mkdir()
        shutil.copy2(SCRIPT, self.root / "Tools/build.sh")
        (self.root / "third_party/clap-wrapper").mkdir(parents=True)
        self.plugins = self.root / "plugins"
        for bundle in BUNDLES:
            folder = self.plugins / bundle
            folder.mkdir(parents=True)
            (folder / "version").write_text("old")
        # Stale output must never turn a failed build into a successful install.
        for relative in ("target/bundled/Jovian-8.clap", "target/bundled/Jovian-8.vst3", "build/au/Jovian-8.component"):
            folder = self.root / relative
            folder.mkdir(parents=True)
            (folder / "version").write_text("stale")
        (self.root / "target/release").mkdir()
        (self.root / "target/release/libjovian_plugin.a").write_text("stale")
        self.bin = self.root / "bin"
        self.bin.mkdir()
        self.tool("cargo", """
echo cargo >> "$JOVIAN_TEST_EVENTS"
[ "$JOVIAN_TEST_FAIL" != cargo ] || exit 42
mkdir -p target/release target/bundled/Jovian-8.clap target/bundled/Jovian-8.vst3
echo new > target/release/libjovian_plugin.a
echo new > target/bundled/Jovian-8.clap/version
echo new > target/bundled/Jovian-8.vst3/version
""")
        self.tool("cmake", """
echo cmake >> "$JOVIAN_TEST_EVENTS"
if [ "$1" = --build ]; then
  [ "$JOVIAN_TEST_FAIL" != au ] || exit 44
  echo new > build/au/Jovian-8.component/version
else
  [ "$JOVIAN_TEST_FAIL" != configure ] || exit 43
fi
""")
        self.tool("codesign", """
echo codesign >> "$JOVIAN_TEST_EVENTS"
[ "$JOVIAN_TEST_FAIL" != sign ] || exit 45
""")
        self.tool("mv", """
case "$1" in
  */.jovian-stage.*/VST3/Jovian-8.vst3)
    [ "$JOVIAN_TEST_FAIL" != install ] || exit 46 ;;
esac
exec /bin/mv "$@"
""")
        self.tool("killall", "exit 0")
        self.tool("auval", '''
echo auval >> "$JOVIAN_TEST_EVENTS"
[ "$JOVIAN_TEST_FAIL" != validation ] || exit 47
if [ "$JOVIAN_TEST_FAIL" = registration ] && [ ! -f "$JOVIAN_TEST_EVENTS.registration" ]; then
  touch "$JOVIAN_TEST_EVENTS.registration"
  echo "FATAL ERROR: didn't find the component"
  exit 2
fi
''')

    def tool(self, name, body):
        path = self.bin / name
        path.write_text("#!/bin/sh\nset -eu\n" + body + "\n")
        path.chmod(0o755)

    def run_build(self, failure="", *args):
        env = dict(os.environ, PATH=f"{self.bin}:/usr/bin:/bin", JOVIAN_PLUGIN_DIR=str(self.plugins),
                   JOVIAN_SIGN_IDENTITY="-", JOVIAN_TEST_FAIL=failure,
                   JOVIAN_TEST_EVENTS=str(self.root / "events"))
        result = subprocess.run(["/bin/zsh", "-f", str(self.root / "Tools/build.sh"), *args], env=env,
                                text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        return result

    def assert_versions(self, expected):
        for bundle in BUNDLES:
            self.assertEqual((self.plugins / bundle / "version").read_text().strip(), expected)

    def assert_no_stage(self):
        self.assertEqual(list(self.plugins.glob(".jovian-stage.*")), [])

    def test_rust_failure_does_not_install_stale_output(self):
        result = self.run_build("cargo")
        self.assertEqual(result.returncode, 42, result.stdout)
        self.assert_versions("old")
        self.assertEqual((self.root / "events").read_text().splitlines(), ["cargo"])

    def test_au_failures_do_not_install_stale_output(self):
        for failure, code in (("configure", 43), ("au", 44)):
            with self.subTest(failure=failure):
                result = self.run_build(failure)
                self.assertEqual(result.returncode, code, result.stdout)
                self.assert_versions("old")
                self.assertNotIn("codesign", (self.root / "events").read_text())

    def test_signing_failure_keeps_previous_plugins(self):
        result = self.run_build("sign")
        self.assertEqual(result.returncode, 45, result.stdout)
        self.assert_versions("old")
        self.assert_no_stage()

    def test_installation_failure_rolls_back_all_formats(self):
        result = self.run_build("install")
        self.assertEqual(result.returncode, 46, result.stdout)
        self.assert_versions("old")
        self.assert_no_stage()

    def test_build_only_never_signs_or_installs(self):
        result = self.run_build("", "--build-only")
        self.assertEqual(result.returncode, 0, result.stdout)
        self.assert_versions("old")
        self.assertNotIn("codesign", (self.root / "events").read_text())

    def test_success_preserves_previous_bundles(self):
        result = self.run_build()
        self.assertEqual(result.returncode, 0, result.stdout)
        self.assert_versions("new")
        backups = list((self.plugins / ".jovian-backups").iterdir())
        self.assertEqual(len(backups), 1)
        for bundle in BUNDLES:
            self.assertEqual((backups[0] / bundle / "version").read_text(), "old")
        self.assert_no_stage()

    def test_validation_failure_is_reported(self):
        result = self.run_build("validation", "--check")
        self.assertEqual(result.returncode, 47, result.stdout)
        self.assert_versions("new")

    def test_validation_waits_for_asynchronous_component_registration(self):
        result = self.run_build("registration", "--check")
        self.assertEqual(result.returncode, 0, result.stdout)
        self.assertEqual((self.root / "events").read_text().splitlines().count("auval"), 2)
        self.assert_versions("new")


if __name__ == "__main__":
    unittest.main()
