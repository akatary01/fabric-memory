"""Own the standalone app's loopback model service."""
import os, pathlib, signal, subprocess, sys, threading, time, urllib.request

resources, data, llm_port = sys.argv[1:]
resources, data = pathlib.Path(resources), pathlib.Path(data)
os.umask(0o077)
children = []
stopping = threading.Event()
lifecycle = threading.RLock()

def stop(*_):
    with lifecycle:
        if stopping.is_set(): return
        stopping.set()
        active=list(children)
    for child in active:
        if child.poll() is None:
            try: child.terminate()
            except (ProcessLookupError, PermissionError): pass

for sig in (signal.SIGTERM, signal.SIGINT): signal.signal(sig, stop)
def watch_parent():
    sys.stdin.buffer.read()
    stop()
threading.Thread(target=watch_parent, daemon=True).start()
logs = data / 'Logs'; logs.mkdir(parents=True, exist_ok=True)
env = {k:v for k,v in os.environ.items() if k in {'PATH','HOME','LANG','LC_ALL','TMPDIR','PYTHONHOME','PYTHONPATH','SSL_CERT_FILE'}}
env.update(OLLAMA_HOST='127.0.0.1:'+llm_port, OLLAMA_MODELS=str(data/'LocalLLM/models'),
           OLLAMA_NO_CLOUD='1', OLLAMA_MAX_LOADED_MODELS='2', OLLAMA_NUM_PARALLEL='1',
           OLLAMA_CONTEXT_LENGTH='4096', PYTHONDONTWRITEBYTECODE='1')
try:
    for name, command, cwd in [
        ('local-model', [str(resources/'ollama/ollama'), 'serve'], resources/'ollama')]:
        with (logs/(name+'.log')).open('ab') as log:
            with lifecycle:
                if stopping.is_set(): raise RuntimeError('App closed during startup')
                children.append(subprocess.Popen(command, env=env, cwd=cwd, stdin=subprocess.DEVNULL,
                                                 stdout=log, stderr=log, start_new_session=True))
    opener=urllib.request.build_opener(urllib.request.ProxyHandler({}))
    for port, path in [(llm_port,'/api/version')]:
        for _ in range(120):
            if stopping.is_set() or any(c.poll() is not None for c in children):
                raise RuntimeError('A bundled service stopped; see Application Support/fabric-memory logs.')
            try:
                with opener.open('http://127.0.0.1:'+port+path,timeout=1) as response:
                    if response.status==200: break
            except Exception: pass
            stopping.wait(.25)
        else: raise RuntimeError('Local services did not become ready; see the app logs.')
    print('ready',flush=True)
    while not stopping.wait(1):
        if any(c.poll() is not None for c in children): break
finally:
    stop()
    for child in children:
        try: child.wait(timeout=5)
        except subprocess.TimeoutExpired:
            try: os.killpg(child.pid,signal.SIGKILL)
            except (ProcessLookupError, PermissionError):
                try: child.kill()
                except (ProcessLookupError, PermissionError): pass
