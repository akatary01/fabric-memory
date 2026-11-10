use qrcodegen::{QrCode, QrCodeEcc};

pub fn svg(text: &str) -> Result<String, String> {
    if text.len() > 2048 {
        return Err("Pairing QR exceeds limit".into());
    }
    let code = QrCode::encode_text(text, QrCodeEcc::Medium)
        .map_err(|_| "Pairing link is too long for a QR")?;
    let mut path = String::new();
    for y in 0..code.size() {
        for x in 0..code.size() {
            if code.get_module(x, y) {
                path.push_str(&format!("M{},{}h1v1h-1z", x + 4, y + 4));
            }
        }
    }
    Ok(format!("<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {0} {0}\" shape-rendering=\"crispEdges\"><rect width=\"100%\" height=\"100%\" fill=\"white\"/><path d=\"{path}\" fill=\"black\"/></svg>",code.size()+8))
}
