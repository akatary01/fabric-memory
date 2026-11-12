import sys,tempfile,unittest
from pathlib import Path
from unittest.mock import patch
sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
from clean import clean,trash

class CleanTests(unittest.TestCase):
    def test_native_trash_move_is_successful_when_enumeration_is_denied(self):
        with tempfile.TemporaryDirectory() as d:
            p=Path(d)/'build';p.mkdir()
            with patch('clean.sys.platform','darwin'),patch('clean.subprocess.run'),patch.object(Path,'iterdir',side_effect=PermissionError('Trash enumeration denied')):
                self.assertEqual(trash(p),'macOS Trash (restore using Finder)')
    def test_preview_preserves_models_and_skips_open_builds(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d)/'project';support=Path(d)/'support'
            for p in [root/'target',root/'.build/mac',root/'assets',root/'.venv',support/'Bin',support/'Sync']:
                p.mkdir(parents=True);(p/'keep').write_text('data')
            result=type('Result',(),{'returncode':0,'stdout':'n'+str(root/'target/keep')+'\n'})()
            with patch('clean.subprocess.run',return_value=result),patch('clean.trash') as trash:
                rows=clean(root,support,dry_run=True);trash.assert_not_called()
            self.assertEqual(next(r for r in rows if r['path']==str(root/'target'))['status'],'skipped')
            self.assertEqual([r['path'] for r in rows if r['status']=='candidate'],[str(root/'.build/mac')])
            self.assertTrue((root/'assets/keep').exists());self.assertTrue((support/'Bin/keep').exists())

if __name__=='__main__':unittest.main()
