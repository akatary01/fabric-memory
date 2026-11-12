import plistlib,sys,tempfile,unittest
from pathlib import Path
from unittest.mock import patch
sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
from cli import main,mac_app,legacy_mac_app

class NativeLauncherTests(unittest.TestCase):
    def test_run_opens_native_bundle_and_does_not_start_git(self):
        root=Path('/test/project');app=Path('/test/fabric-memory.app')
        with patch('sys.argv',['fabric-memory','run']),patch('cli.sys.platform','darwin'),patch('cli.environment',return_value={'FABRIC_DATA_DIR':'/test/data'}),patch('cli.mac_app',return_value=app),patch('cli.subprocess.run') as run,patch('cli.open_mac_app') as launch:
            main(root)
        calls=[c.args[0] for c in run.call_args_list]
        launch.assert_called_once_with(app,{'FABRIC_DATA_DIR':'/test/data'})
        self.assertFalse(any('launch-git' in c for c in calls))

    def test_current_applications_bundle_is_reused(self):
        app=Path('/Applications/Fabric Memory.app')
        with patch('cli.sys.platform','darwin'),patch('cli.installed_mac_app',return_value=app),patch('cli.subprocess.run') as run:
            self.assertEqual(mac_app(Path('/test/project'),{}),app)
        run.assert_not_called()

    def test_legacy_name_migration_recognizes_only_fabric(self):
        with tempfile.TemporaryDirectory() as folder:
            legacy = Path(folder) / 'fabric-memory.app'
            destination = Path(folder) / 'Fabric Memory.app'
            info = legacy / 'Contents/Info.plist'
            info.parent.mkdir(parents=True)
            info.write_bytes(plistlib.dumps({'CFBundleIdentifier': 'other.app'}))
            self.assertIsNone(legacy_mac_app(destination))
            info.write_bytes(plistlib.dumps({'CFBundleIdentifier': 'local.memoryfabric.mac'}))
            self.assertEqual(legacy_mac_app(destination), legacy)

if __name__=='__main__' :unittest.main()
