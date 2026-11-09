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
  function markdown(text) {
    const container = document.createElement('div'); container.className = 'markdown';
    const html = marked.parse(text, { gfm: true, breaks: true });
    container.append(DOMPurify.sanitize(html, {
      RETURN_DOM_FRAGMENT: true,
      ALLOWED_TAGS: ['p','br','strong','em','del','code','pre','blockquote','ul','ol','li','h1','h2','h3','h4','h5','h6','hr','table','thead','tbody','tr','th','td','a'],
      ALLOWED_ATTR: ['href','title','start'],
      ALLOW_DATA_ATTR: false, ALLOW_ARIA_ATTR: false,
    }));
    container.addEventListener('click', event => {
      const link = event.target.closest('a'); if (!link) return;
      event.preventDefault();
      let url; try { url = new URL(link.getAttribute('href')); } catch { return; }
      if (!['http:', 'https:'].includes(url.protocol)) return;
      window.__TAURI__?.core.invoke('open_web_source', { url: url.href }).catch(error => {
        const notice = document.getElementById('message');
        notice.textContent = String(error); notice.className = 'error'; notice.hidden = false;
      });
    });
    return container;
  }
  function render(turn) {
    const fragment = document.createDocumentFragment(); const parts = split(turn);
    if (parts.thinking) {
      const details = document.createElement('details'); details.className = 'thinking';
      const summary = document.createElement('summary'); summary.textContent = 'Thinking';
      details.append(summary, markdown(parts.thinking)); fragment.append(details);
    }
    if (parts.answer) fragment.append(markdown(parts.answer));
    return fragment;
  }
  window.FabricChat = { split, markdown, render };
})();
