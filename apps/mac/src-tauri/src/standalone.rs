//! Relocatable release setup. No source checkout or system Python is required.
use std::{io::{BufRead, BufReader}, path::PathBuf, process::{Child, Command, Stdio}};

pub fn prepare() -> Result<Option<Child>, Box<dyn std::error::Error>> {
    let executable = std::env::current_exe()?;
    let resources = executable.parent().ok_or("Missing app directory")?.join("../Resources");
    if !resources.join("standalone.json").is_file() { return Ok(None); }
    let resources = resources.canonicalize()?;
    let arch = if cfg!(target_arch="aarch64") {"arm64"} else {"x86_64"};
    let runtime = resources.join("runtimes").join(arch);
    let data = std::env::var_os("FABRIC_DATA_DIR").map(PathBuf::from).unwrap_or_else(||
        PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join("Library/Application Support/local.memoryfabric.mac"));
    std::fs::create_dir_all(&data)?;
    std::env::set_var("FABRIC_DATA_DIR", &data);
    std::env::set_var("FABRIC_ASSET_DIR", resources.join("assets"));
    std::env::set_var("ORT_DYLIB_PATH", runtime.join("libonnxruntime.dylib"));
    std::env::set_var("FABRIC_PYTHON", runtime.join("python/bin/python3"));
    std::env::set_var("PYTHONHOME", runtime.join("python"));
    std::env::set_var("PYTHONDONTWRITEBYTECODE", "1");
    std::env::set_var("PYTHONPATH", runtime.join("site-packages"));
    std::env::set_var("SSL_CERT_FILE", runtime.join("site-packages/certifi/cacert.pem"));
    // Keep model downloads writable, while the signed application stays immutable.
    let models = data.join("LocalLLM/models");
    std::fs::create_dir_all(models.join("blobs"))?;
    let seed = resources.join("models");
    for file in std::fs::read_dir(seed.join("blobs"))? {
        let file = file?; let target = models.join("blobs").join(file.file_name());
        if !target.exists() {
            if !Command::new("/bin/cp").arg("-c").arg(file.path()).arg(&target).status()?.success() {
                std::fs::copy(file.path(), &target)?;
            }
        }
    }
    let manifest = models.join("manifests/registry.ollama.ai/library/lfm3-vl");
    std::fs::create_dir_all(&manifest)?;
    let tag="latest";
    if !manifest.join(tag).exists() { std::fs::copy(seed.join("manifests/registry.ollama.ai/library/lfm3-vl").join(tag),manifest.join(tag))?; }
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    std::env::set_var("FABRIC_LOCAL_ENDPOINT", format!("http://127.0.0.1:{port}"));
    drop(listener);
    let mut child=Command::new(runtime.join("python/bin/python3"))
        .args(["-B", "-c", include_str!("../../../../tools/packaged_services.py")])
        .arg(&resources).arg(&data).arg(port.to_string())
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::inherit()).spawn()?;
    let mut line=String::new();
    BufReader::new(child.stdout.take().ok_or("Missing service output")?).read_line(&mut line)?;
    if line.trim()!="ready" { return Err("Bundled model service could not start. See the app Logs folder.".into()); }
    Ok(Some(child))
}
