/* Locally bundled Markdown and sanitizer; no remote scripts or image requests. */
(() => {
  function split(turn) {
    const thoughts = [];
    if (typeof turn.thinking === 'string' && turn.thinking.trim()) thoughts.push(turn.thinking.trim());
    // Support older saved replies and models that emit reasoning inline. An
    // unfinished thinking block stays hidden rather than spilling into the reply.
    const answer = String(turn.content ?? '').replace(/<(think|thinking)\b[^>]*>([\s\S]*?)(?:<\/\1\s*>|$)/gi,
      (_, tag, text) => { if (text.trim()) thoughts.push(text.trim()); return ''; }).trim();
    return { answer, thinking: thoughts.join('\n\n') };
  }
  function copyButton(text,label='Copy'){
    const button=document.createElement('button');button.type='button';button.className='text-button copy-output';button.textContent=label;button.setAttribute('aria-label',label);
    button.addEventListener('click',async()=>{try{await window.__TAURI__.core.invoke('app_command',{command:'clipboard_write',args:{text}});button.textContent='Copied';}catch(error){button.textContent='Copy failed';button.title=String(error?.message??error);}setTimeout(()=>{button.textContent=label;},2000);});return button;
  }
  function markdown(text) {
    const container = document.createElement('div'); container.className = 'markdown';
    const html = marked.parse(text, { gfm: true, breaks: true });
    container.append(DOMPurify.sanitize(html, {
      RETURN_DOM_FRAGMENT: true,
      ALLOWED_TAGS: ['p','br','strong','em','del','code','pre','blockquote','ul','ol','li','h1','h2','h3','h4','h5','h6','hr','table','thead','tbody','tr','th','td','a'],
      ALLOWED_ATTR: ['href','title','start'],
      ALLOW_DATA_ATTR: false, ALLOW_ARIA_ATTR: false,
    }));
    for(const pre of container.querySelectorAll('pre')){const wrapper=document.createElement('section');wrapper.className='code-output';const bar=document.createElement('div');bar.className='code-toolbar';bar.append(copyButton(pre.querySelector('code')?.textContent??pre.textContent,'Copy code'));pre.before(wrapper);wrapper.append(bar,pre);}
    container.addEventListener('click', event => {
      const link = event.target.closest('a'); if (!link) return;
      event.preventDefault();
      let url; try { url = new URL(link.getAttribute('href')); } catch { return; }
      if (!['http:', 'https:'].includes(url.protocol)) return;
      window.__TAURI__?.core.invoke('app_command', { command: 'open_web_source', args: { url: url.href } }).catch(error => {
        const notice = document.getElementById('message');
        notice.textContent = String(error); notice.className = 'error'; notice.hidden = false;
      });
    });
    return container;
  }
  const PREVIEW_POLICY = "default-src 'none'; script-src 'none'; style-src 'unsafe-inline'; img-src data:; font-src 'none'; connect-src 'none'; frame-src 'none'; object-src 'none'; form-action 'none'; base-uri 'none'";
  function previewDocument(source) {
    const clean = DOMPurify.sanitize(source, {
      WHOLE_DOCUMENT: true,
      FORBID_TAGS: ['script','iframe','frame','frameset','object','embed','base','meta','link','form','audio','video','source','portal'],
      FORBID_ATTR: ['href','xlink:href','srcdoc','action','formaction','target','download','ping','srcset'],
      ALLOWED_URI_REGEXP: /^(?:#|data:image\/(?:png|jpeg|gif|webp);)/i,
    });
    const doc = new DOMParser().parseFromString(clean, 'text/html');
    // Always put our policy first. Model HTML cannot replace it or call native IPC.
    return '<!doctype html><html><head><meta charset="utf-8"><meta http-equiv="Content-Security-Policy" content="'+PREVIEW_POLICY+'"><meta name="viewport" content="width=device-width, initial-scale=1"><style>html{color-scheme:light}body{margin:0;min-height:100vh;font-family:system-ui,sans-serif}*{box-sizing:border-box}</style>'+doc.head.innerHTML+'</head><body>'+doc.body.innerHTML+'</body></html>';
  }
  function preview(source, index) {
    const card = document.createElement('section'); card.className = 'html-preview';
    const toolbar = document.createElement('div'); toolbar.className = 'preview-toolbar';
    const title = document.createElement('span'); title.textContent = 'Mockup';
    const toggle = document.createElement('button'); toggle.type = 'button'; toggle.textContent = 'View HTML'; toggle.setAttribute('aria-expanded','false');
    const frame = document.createElement('iframe'); frame.title = `HTML mockup ${index}`;
    frame.setAttribute('sandbox',''); frame.setAttribute('referrerpolicy','no-referrer');
    frame.setAttribute('allow', "camera 'none'; microphone 'none'; geolocation 'none'; clipboard-read 'none'; clipboard-write 'none'");
    const code = document.createElement('pre'); code.className = 'preview-source'; code.hidden = true;
    const content = document.createElement('code'); content.textContent = source; code.append(content);
    if (source.length <= 131072) frame.srcdoc = previewDocument(source);
    else { frame.hidden = true; code.hidden = false; toggle.disabled = true; title.textContent = 'Mockup exceeds preview limit'; }
    toggle.addEventListener('click', () => {const showCode=code.hidden;code.hidden=!showCode;frame.hidden=showCode;toggle.textContent=showCode?'Show preview':'View HTML';toggle.setAttribute('aria-expanded',String(showCode));});
    toolbar.append(title,toggle,copyButton(source,'Copy HTML'));card.append(toolbar,frame,code);return card;
  }
  function render(turn) {
    const fragment = document.createDocumentFragment(); const parts = split(turn);
    if (!parts.answer) return fragment;
    if (/^\s*(?:<!doctype\s+html|<html\b)/i.test(parts.answer)) {fragment.append(preview(parts.answer,1));return fragment;}
    let buffer='', count=0;
    for (const token of marked.lexer(parts.answer,{gfm:true})) {
      if(token.type==='code' && /^(?:html|htm)(?:\s|$)/i.test(token.lang||'') && count<3) {
        if(buffer.trim()) fragment.append(markdown(buffer));buffer='';fragment.append(preview(token.text,++count));
      } else buffer+=token.raw;
    }
    if(buffer.trim()) fragment.append(markdown(buffer));return fragment;
  }
  window.FabricChat = { copyButton, split, markdown, render, previewDocument };
})();
