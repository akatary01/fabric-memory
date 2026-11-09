const $ = id => document.getElementById(id);
let allMemories = [];
let searchActive = false;
let totalMemories = 0;
let messageTimer;
let chatBusy = false;
let cloudAvailable = false;

for (const [id, values] of [['answer-mode', ['auto','local','cloud']], ['web-mode', ['auto','off','on']]]) {
  try { const value = localStorage.getItem(`fabric-${id}`); if (values.includes(value)) $(id).value = value; } catch {}
  $(id).addEventListener('change', () => {
    try { localStorage.setItem(`fabric-${id}`, $(id).value); } catch {}
    updateRoutingHint();
  });
}
function updateRoutingHint() {
  const mode = $('answer-mode').value, web = $('web-mode').value;
  const model = mode === 'local' ? 'The answer stays on this Mac; cloud fallback is disabled.' :
    mode === 'cloud' ? (cloudAvailable ? 'Cloud receives the prompt, selected memory and recent conversation.' : 'Cloud needs OPENAI_API_KEY in apps/mac/.env and an app restart.') :
    'Auto prefers local for simple tasks; cloud may receive your prompt, selected memory and recent conversation for demanding tasks.';
  $('routing-hint').textContent = `${model} ${web === 'off' ? 'Web requests are off.' : 'Web search sends your question to search engines; selected page text reaches the answering model.'}`;
}

function renderConversation(turns) {
  const transcript = $('conversation');
  if (!turns.length) return;
  transcript.replaceChildren();
  for (const turn of turns) {
    const row = document.createElement('article'); row.className = 'chat-turn';
    const role = document.createElement('strong'); role.textContent = turn.role === 'user' ? 'You' : turn.provider === 'cloud' ? 'Cloud' : turn.provider === 'local' ? 'This Mac' : 'Assistant';
    row.append(role);
    if (turn.role === 'user') {
      const content = document.createElement('p'); content.textContent = turn.content; row.append(content);
    } else { row.append(window.FabricChat.render(turn)); }
    if (turn.model || turn.route_reason) {
      const meta = document.createElement('small'); meta.className = 'turn-meta';
      meta.textContent = [turn.model, turn.route_reason].filter(Boolean).join(' · '); row.append(meta);
    }
    if (Array.isArray(turn.sources) && turn.sources.length) {
      const sources = document.createElement('div'); sources.className = 'chat-sources';
      turn.sources.forEach((source, index) => {
        let url; try { url = new URL(source.url); } catch { return; }
        if (!['http:', 'https:'].includes(url.protocol)) return;
        const link = document.createElement('a'); link.href = url.href;
        link.textContent = `[S${index + 1}] ${source.title || url.hostname} · ${source.kind}`;
        link.addEventListener('click', event => {
          event.preventDefault(); invoke('open_web_source', { url: url.href }).catch(error => message(String(error), true));
        });
        sources.append(link);
      });
      row.append(sources);
    }
    transcript.append(row);
  }
  transcript.scrollTop = transcript.scrollHeight;
}

async function loadConversation() {
  const state = await invoke('conversation');
  cloudAvailable = state.cloud_available;
  $('model-status').textContent = `${state.model} · ${cloudAvailable ? `cloud: ${state.cloud_model}` : 'cloud key not configured'}`;
  updateRoutingHint();
  renderConversation(state.turns);
}

function invoke(command, args = {}) {
  if (!window.__TAURI__?.core?.invoke) throw new Error('Open the Mac app with npm run mac to connect to your local memory engine.');
  return window.__TAURI__.core.invoke(command, args);
}

function message(text, error = false) {
  clearTimeout(messageTimer);
  $('message').textContent = text;
  $('message').className = error ? 'error' : '';
  $('message').hidden = false;
  if (!error) messageTimer = setTimeout(() => { $('message').hidden = true; }, 7000);
}

async function action(button, operation) {
  const title = button.textContent;
  button.disabled = true;
  button.textContent = 'Working…';
  try { await operation(); }
  catch (error) { message(String(error?.message ?? error), true); }
  finally { button.disabled = false; button.textContent = title; }
}

function empty(title, description) {
  const box = document.createElement('div'); box.className = 'empty-state';
  const symbol = document.createElement('span'); symbol.className = 'empty-symbol'; symbol.textContent = '◈'; symbol.setAttribute('aria-hidden', 'true');
  const heading = document.createElement('h3'); heading.textContent = title;
  const copy = document.createElement('p'); copy.textContent = description;
  box.append(symbol, heading, copy);
  return box;
}

function render(memories) {
  const list = $('memories'); list.replaceChildren();
  $('list-title').firstChild.textContent = searchActive ? 'Closest memories ' : 'Your memories ';
  $('memory-count').textContent = !searchActive && totalMemories > memories.length ? `${memories.length} of ${totalMemories}` : memories.length;
  $('clear-search').hidden = !searchActive;
  if (!memories.length) {
    list.append(empty(searchActive ? 'No memories to search yet' : 'Make room for your first memory',
      searchActive ? 'Save a note or import memories from your iPhone.' : 'Add a note above. You can find it later using your own words.'));
    return;
  }
  const details = new Map(allMemories.map(memory => [memory.memory, memory]));
  for (const memory of memories) {
    const info = details.get(memory.memory) ?? memory;
    const card = document.createElement('article'); card.className = 'memory-card';
    const top = document.createElement('div'); top.className = 'memory-top';
    const badge = document.createElement('span'); badge.className = info.translated ? 'badge translated' : 'badge';
    badge.textContent = info.translated ? 'MiniLM → MPNet · translated' : 'MPNet · native';
    top.append(badge);
    if (typeof memory.score === 'number') {
      const score = document.createElement('span'); score.className = 'score';
      score.textContent = `Similarity ${memory.score.toFixed(3)}`;
      top.append(score);
    }
    const remove = document.createElement('button'); remove.className = 'delete'; remove.textContent = 'Forget';
    remove.setAttribute('aria-label', `Forget memory: ${memory.text.slice(0, 60)}`);
    remove.addEventListener('click', () => action(remove, async () => {
      await invoke('forget', { memory: memory.memory });
      await refresh();
      if (searchActive) await performSearch();
      message('Memory forgotten. Its deletion will be included in your next export.');
    }));
    top.append(remove);
    const text = document.createElement('p'); text.className = 'memory-text'; text.textContent = memory.text;
    card.append(top, text); list.append(card);
  }
}

async function refresh() {
  const [status, memories] = await Promise.all([invoke('status'), invoke('list_memories')]);
  $('auto-sync-status').textContent = status.automatic_sync || 'Automatic memory sync enabled.';
  allMemories = memories;
  totalMemories = status.live_memories;
  $('nav-count').textContent = status.live_memories;
  $('connection').textContent = 'Local engine ready';
  $('connection-dot').classList.add('ready');
  $('ledger-status').textContent = `${status.live_memories} ${status.live_memories === 1 ? 'memory' : 'memories'} · ${status.events} durable ${status.events === 1 ? 'event' : 'events'}`;
  if (!searchActive) render(allMemories);
}

async function performSearch() {
  const text = $('query').value.trim();
  if (!text) { searchActive = false; render(allMemories); return; }
  const hits = await invoke('search', { text });
  searchActive = true;
  render(hits);
}

$('memory-form').addEventListener('submit', event => {
  event.preventDefault();
  const text = $('memory-text').value.trim();
  if (!text) return;
  action(event.submitter, async () => {
    await invoke('remember', { memory: `note-${crypto.randomUUID()}`, text });
    $('memory-text').value = '';
    searchActive = false;
    await refresh();
    message('Memory saved and embedded on this Mac.');
  });
});
$('search-form').addEventListener('submit', event => { event.preventDefault(); action(event.submitter, performSearch); });
$('clear-search').addEventListener('click', () => { searchActive = false; $('query').value = ''; render(allMemories); });
$('refresh').addEventListener('click', () => action($('refresh'), async () => { await refresh(); if (searchActive) await performSearch(); }));
$('import').addEventListener('click', () => $('import-file').click());
$('import-file').addEventListener('change', async event => {
  const file = event.target.files[0]; if (!file) return;
  await action($('import'), async () => {
    if (file.size > 32 * 1024 * 1024) throw new Error('Please choose an export smaller than 32 MB.');
    const inserted = await invoke('import_memories', { payload: await file.text() });
    searchActive = false;
    await refresh();
    message(inserted ? `Imported ${inserted} new events. Compatible foreign vectors are translated locally.` : 'No new events. This export has already been imported.');
  });
  event.target.value = '';
});
$('export').addEventListener('click', () => action($('export'), async () => {
  await invoke('export_memory_file');
  message('Event ledger exported. The file is selected in Finder.');
}));
$('pair').addEventListener('click',()=>action($('pair'),async()=>{
  const url = await invoke('pair_iphone');message(`Mac address: ${url}. AirDrop the selected pairing file to your iPhone and select it with Connect Mac. Use the same Wi-Fi and keep this file private.`);
}));
$('sync').addEventListener('click',()=>action($('sync'),async()=>{
  const result=await invoke('sync_memories');await refresh();message(`Git sync complete: ${result.uploaded} events uploaded, ${result.downloaded} downloaded.`);
}));
$('prompt-form').addEventListener('submit', event => {
  event.preventDefault();
  const text = $('prompt').value.trim();
  if (!text || chatBusy) return;
  chatBusy = true;
  const mode = $('answer-mode').value, web = $('web-mode').value;
  $('answer-mode').disabled = true; $('web-mode').disabled = true;
  $('generation-status').textContent = 'Choosing the model, retrieving context and preparing your answer…';
  action(event.submitter, async () => {
    try {
      const result = await invoke('send_prompt', { text, mode, web });
      $('prompt').value = '';
      await loadConversation();
      const speed = typeof result.tokens_per_second === 'number' ? ` · ${result.tokens_per_second.toFixed(1)} tokens/s` : '';
      $('generation-status').textContent = `${result.provider} · ${result.memories} memories (${result.translated_memories} translated) · ${result.sources.length} web sources${speed}. ${result.route_reason}${result.incomplete ? ' Reply reached the output limit.' : ''}`;
      await refresh();
    } catch (error) {
      $('generation-status').textContent = 'Could not answer. Your prompt is kept above.';
      throw error;
    } finally { chatBusy = false; $('answer-mode').disabled = false; $('web-mode').disabled = false; }
  });
});
setInterval(()=>refresh().catch(()=>{}),10000);

loadConversation().catch(error => message(String(error?.message ?? error), true));

refresh().catch(error => {
  $('connection').textContent = 'Engine unavailable';
  $('memories').replaceChildren(empty('Could not connect', 'Check the message above and try Refresh.'));
  message(String(error?.message ?? error), true);
});
