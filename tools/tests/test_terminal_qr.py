import io,sys,tempfile,unittest
from contextlib import redirect_stdout
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
from qr import terminal
from beacon_launch import pairing_page

class TerminalQRTests(unittest.TestCase):
    def test_compact_pairing_is_text_and_fits_a_small_terminal(self):
        uri='https://akatary.com/fabric-git#join='+'a'*32
        rows=terminal(uri,ansi=False).splitlines()
        self.assertLessEqual(max(map(len,rows)),35)
        self.assertLessEqual(len(rows),18)
        self.assertTrue(set(''.join(rows))<=set(' ▀▄█'))
        self.assertTrue(any('▀' in row or '▄' in row for row in rows))
        self.assertNotIn('\033',terminal(uri,ansi=True))
    def test_launch_prints_qr_without_picture_files(self):
        class Invitations:
            def invite(self):return {'uri':'https://akatary.com/fabric-git#join='+'a'*32,'invitation':'a'*32}
        with tempfile.TemporaryDirectory() as d:
            output=io.StringIO()
            with redirect_stdout(output):pairing_page(Path(d),{'name':'Test beacon'},Invitations())
            self.assertIn('█',output.getvalue());self.assertIn('Invitation code:',output.getvalue())
            self.assertEqual(list(Path(d).iterdir()),[])

if __name__=='__main__':unittest.main()
