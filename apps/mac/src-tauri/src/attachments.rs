use base64::Engine;
use serde_json::{json, Value};
use std::io::Write;
use std::process::{Command, Stdio};

const MAX_FILE: usize = 6 * 1024 * 1024;
const DOC_CHARS: usize = 12_000;

pub struct Attachments {
    pub names: Vec<String>,
    pub documents: Vec<Value>,
    pub images: Vec<String>,
}
impl Attachments {
    pub fn read(items: &[Value]) -> Result<Self,String> {
        if items.is_empty() || items.len()>3 { return Err("Attach between one and three files.".into()); }
        let mut result=Self{names:vec![],documents:vec![],images:vec![]};
        let mut total=0;
        for item in items {
            let name=item["name"].as_str().filter(|name|!name.is_empty() && name.len()<=512 && !name.contains(['/', '\\']) && !name.chars().any(char::is_control)).ok_or("Invalid attachment name.")?;
            let data=item["data"].as_str().filter(|data|data.len()<=MAX_FILE.div_ceil(3)*4).ok_or("An attachment exceeds 6 MB.")?;
            let bytes=base64::engine::general_purpose::STANDARD.decode(data).map_err(|_|"Could not read attachment bytes.")?;
            total+=bytes.len();
            if bytes.is_empty() || bytes.len()>MAX_FILE || total>MAX_FILE {return Err("Attachments must be nonempty and total at most 6 MB.".into());}
            let ext=name.rsplit_once('.').map(|(_,ext)|ext.to_ascii_lowercase()).unwrap_or_default();
            if matches!(ext.as_str(),"png"|"jpg"|"jpeg"|"webp") {
                let valid=match ext.as_str(){
                    "png"=>bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
                    "jpg"|"jpeg"=>bytes.starts_with(&[0xff,0xd8,0xff]),
                    _=>bytes.starts_with(b"RIFF") && bytes.get(8..12)==Some(b"WEBP"),
                };
                if !valid {return Err(format!("{name} is not a valid image of that type."));}
                result.images.push(data.into());
            } else {
                let content=match ext.as_str() {
                    "pdf"=>crate::computer::pdf_text(data)?,
                    "docx"=>docx_text(&bytes)?,
                    "txt"|"md"|"csv"|"json"|"log"|"tsv"|"xml"|"yaml"|"yml"=>String::from_utf8(bytes).map_err(|_|format!("{name} must use UTF-8 text."))?,
                    _=>return Err(format!("{name}: attach PNG, JPEG, WebP, PDF, DOCX or a plain text document.")),
                };
                if content.trim().is_empty(){return Err(format!("No readable text in {name}. Scanned pages can be attached as images."));}
                result.documents.push(json!({"name":name,"text":content.chars().take(DOC_CHARS).collect::<String>(),"truncated":content.chars().nth(DOC_CHARS).is_some()}));
            }
            result.names.push(name.into());
        }
        Ok(result)
    }
}
fn docx_text(bytes: &[u8]) -> Result<String,String> {
    let code=r#"
import io, sys, zipfile, xml.etree.ElementTree as ET
try:
    with zipfile.ZipFile(io.BytesIO(sys.stdin.buffer.read())) as doc:
        file = doc.getinfo('word/document.xml')
        if file.file_size > 4_000_000: raise ValueError()
        xml = doc.read(file)
    if b'<!DOCTYPE' in xml or b'<!ENTITY' in xml: raise ValueError()
    ns = '{http://schemas.openxmlformats.org/wordprocessingml/2006/main}'
    result = '\n'.join(''.join(t.text or '' for t in p.iter(ns+'t')) for p in ET.fromstring(xml).iter(ns+'p'))
    sys.stdout.buffer.write(result[:12001].encode('utf-8'))
except (ValueError, KeyError, OSError, ET.ParseError, zipfile.BadZipFile):
    sys.exit(1)
"#;
    let mut child=Command::new(std::env::var("FABRIC_PYTHON").unwrap_or_else(|_|"python3".into()))
        .args(["-B","-c",code]).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null())
        .spawn().map_err(|_|"Local document reader is unavailable.")?;
    child.stdin.take().ok_or("Document reader input unavailable.")?.write_all(bytes).map_err(|_|"Could not read DOCX.")?;
    let output=child.wait_with_output().map_err(|_|"Could not read DOCX.")?;
    if !output.status.success(){return Err("Could not extract text from this DOCX.".into());}
    String::from_utf8(output.stdout).map_err(|_|"Invalid DOCX text.".into())
}
