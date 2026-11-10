"""Compact terminal QR output, using the same half-block layout as Expo."""
from vendor.qrcodegen import QrCode

def terminal(text,ansi=None):
    # Light modules use the terminal's foreground; dark modules use its existing
    # background. No ANSI background colors, image files, or image protocols.
    if not isinstance(text,str) or len(text.encode())>2048:raise ValueError('Pairing QR exceeds limit')
    qr=QrCode.encode_text(text,QrCode.Ecc.LOW);size=qr.get_size()
    rows=['▄'*(size+2)]
    pixels={(False,False):'█',(False,True):'▀',(True,False):'▄',(True,True):' '}
    for y in range(0,size,2):
        rows.append('█'+''.join(pixels[(qr.get_module(x,y),qr.get_module(x,y+1))] for x in range(size))+'█')
    return '\n'.join(rows)
