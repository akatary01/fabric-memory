const $ = id => document.getElementById(id);
let allMemories = [];
let searchActive = false;
let totalMemories = 0;
let messageTimer;
let chatBusy = false;
let cloudAvailable = false;
let conversationState = { turns: [] };
let beaconOnline=false;
const conversationDrafts=new Map();
let closePaperMenu=()=>{};
let switchingConversation=false;
let taskPoll,taskState={running:false};
let computerEnabled=false;
let taskExpanded=false;

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
    mode === 'cloud' ? (cloudAvailable ? 'Cloud receives the prompt, selected memory and recent conversation.' : 'Add your API token in Settings → Cloud.') :
    'Auto prefers local for simple tasks; cloud may receive your prompt, selected memory and recent conversation for demanding tasks.';
  renderContext();
  $('routing-hint').textContent = `${model} ${web === 'off' ? 'Web requests are off.' : 'Web search sends your question to search engines; selected page text reaches the answering model.'}`;
}

function renderConversation(turns) {
  const transcript = $('conversation');
  if (!turns.length) {transcript.replaceChildren();return;}
  transcript.replaceChildren();
  for (const turn of turns) {
    const row = document.createElement('article'); row.className = 'chat-turn'; row.dataset.role = turn.role;row.dataset.messageText=turn.role==='user'||turn.failure?turn.content:window.FabricChat.split(turn).answer;
    const role = document.createElement('strong'); role.textContent = turn.role === 'user' ? 'You' : turn.provider === 'cloud' ? 'Cloud' : turn.provider === 'local' ? 'This Mac' : 'Assistant';
    row.append(role);
    if (turn.role === 'user') {
      const content = document.createElement('p'); content.textContent = turn.content; row.append(content);
     } else if(turn.failure){const content=document.createElement('p');content.className='failure-explanation';content.textContent=turn.content;row.append(content);const actions=document.createElement('div');actions.className='toolbar';const add=(label,handler)=>{const button=document.createElement('button');button.className='text-button';button.textContent=label;button.addEventListener('click',handler);actions.append(button);};const draft=value=>{if(chatBusy)return;$('prompt').value=value;resizePrompt();$('prompt').focus();};add('Add details',()=>draft('Here is more information about the failure: '));add('Edit request',()=>draft(turn.original_prompt||''));if(turn.failure.connections)add('Open Connections',()=>selectView('connections'));if(turn.failure.settings)add('Open Settings',()=>{selectView('settings');settingsTab('cloud');});if(turn.failure.local)add('Use Local',()=>{if(chatBusy)return;$('answer-mode').value='local';updateRoutingHint();draft('Continue the previous task locally. Check what already completed before repeating any action.');});row.append(actions);
    } else { row.append(window.FabricChat.render(turn)); }
    if (turn.model || turn.route_reason) {
      const meta = document.createElement('small'); meta.className = 'turn-meta';
      meta.textContent = [turn.provider==='cloud'?'Cloud':turn.provider==='local'?'This Mac':'',turn.model].filter(Boolean).join(' · ');meta.title=turn.route_reason||''; row.append(meta);
    }
    if (Array.isArray(turn.sources) && turn.sources.length) {
      const sources = document.createElement('div'); sources.className = 'chat-sources';
      turn.sources.forEach((source, index) => {
        let url; try { url = new URL(source.url); } catch { return; }
        if (!['http:', 'https:'].includes(url.protocol)) return;
        const link = document.createElement('a'); link.href = url.href;
        link.textContent = `[S${index + 1}] ${source.title || url.hostname} · ${source.kind}`;
        link.addEventListener('click', event => {
          event.preventDefault(); invoke('open_web_source',{url:url.href}).catch(error=>message(error.message,true));
        });
        sources.append(link);
      });
      row.append(sources);
    }
    const lastRun=(turn.steps||[]).filter(s=>s.kind==='done'&&s.tool==='run_code'&&typeof s.result?.exit_code==='number').at(-1);
    if(lastRun){const details=document.createElement('details');details.className='command-receipt';const title=document.createElement('summary');title.textContent='Last command · exit '+lastRun.result.exit_code;const pre=document.createElement('pre');pre.className='tool-result';pre.textContent=[lastRun.result.stdout,lastRun.result.stderr].filter(Boolean).join('\n');details.append(title,window.FabricChat.copyButton(pre.textContent,'Copy output'),pre);row.append(details);}
    const copyBar=document.createElement('div');copyBar.className='message-output-actions';copyBar.append(window.FabricChat.copyButton(row.dataset.messageText,turn.role==='user'?'Copy message':'Copy reply'));row.append(copyBar);
    transcript.append(row);
  }
  requestAnimationFrame(()=>{transcript.scrollTop = transcript.scrollHeight;});
}

async function loadConversation() {
  const state = await invoke('conversation');
  conversationState = state;
  if(!document.querySelector('[data-panel=chat]').hidden)$('active-view-title').textContent=state.title;
  renderConversationList(state.conversations||[]);
  $('deleted-conversations').hidden=!state.deleted?.length;
  renderDeletedConversations();
  $('prompt').value=conversationDrafts.get(state.id)||'';
  resizePrompt();
  cloudAvailable = state.cloud_available;
  $('cloud-connection-state').textContent=cloudAvailable?'Available':'Not configured';
  $('local-model-name').textContent=state.model+' · This Mac';
  $('cloud-model-name').textContent=cloudAvailable?state.cloud_model:'Optional cloud answers';

  updateRoutingHint();
  renderConversation(state.turns);
  renderActivity(state.turns);
  renderContext();
}

function invoke(command, args = {}) {
  if(!window.__TAURI__?.core?.invoke)return Promise.reject(new Error('Open Memory Fabric with fabric-memory run.'));
  return window.__TAURI__.core.invoke('app_command',{command,args});
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
  button.textContent = button.closest('#prompt-form') ? '…' : 'Working…';
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
    const card = document.createElement('article'); card.className = 'memory-card';card.dataset.memoryId=memory.memory;
    const top = document.createElement('div'); top.className = 'memory-top';
    const badge = document.createElement('span'); badge.className = info.translated ? 'badge translated' : 'badge';
    badge.textContent = info.translated ? 'MiniLM → MPNet · translated' : 'MPNet · native';
    top.append(badge);
    const access=document.createElement('span');access.className='access-badge';const scope=(info.access?.scope||'public')==='public'?'public':'private';access.textContent=scope[0].toUpperCase()+scope.slice(1);access.title=info.access?.reason||'Previously shared memory';top.append(access);
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
  $('auto-sync-status').textContent = status.automatic_sync?.startsWith('Sync waiting:')?'Waiting for your Memory beacon.':status.automatic_sync || 'Automatic memory sync enabled.';$('auto-sync-status').title=status.automatic_sync||'';
  beaconOnline=status.beacon_connected===true;
  $('beacon-warning').hidden=beaconOnline;
  $('beacon-connection-badge').textContent=beaconOnline?'Connected':'Not connected';
  allMemories = memories;
  totalMemories = status.live_memories;
  $('nav-count').textContent = status.live_memories;
  $('connection').textContent = 'Local engine ready';
  $('connection-dot').classList.add('ready');
  const usage=status.usage;if(usage){$('usage-status').textContent=`Local: ${usage.local_calls||0} · Cloud: ${usage.cloud_calls} · Est. saved: ${usage.priced?'$'+usage.estimated_saved_usd.toFixed(4):'—'}`;$('usage-status').title=usage.note;$('usage-details').textContent=`${usage.local_calls||0} local calls · ${usage.cloud_calls} cloud calls · ${usage.local_answers} local answers since tracking began · ${usage.historical_cloud_replies||0} earlier cloud replies included · Estimated priced cloud usage $${usage.estimated_cloud_usd.toFixed(4)}${usage.unpriced_answers?` · ${usage.unpriced_answers} requests without known pricing`:""}. ${usage.note} Baseline: ${usage.baseline_model}.`;}
  $('ledger-status').textContent = `${status.live_memories} ${status.live_memories === 1 ? 'memory' : 'memories'} · ${status.events} durable ${status.events === 1 ? 'event' : 'events'}`;
  if (!searchActive) render(allMemories);
  renderContext();
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
  await invoke('export_memory_file');message('Event ledger exported and selected in Finder. Keep this file private.');
}));
$('pair').addEventListener('click',()=>action($('pair'),async()=>{
  const state=await loadBeacon();if(!state.connected){$('beacon-settings').showModal();return;}
  const invitation=await invoke('beacon_invite');await showQR(invitation.uri,invitation.expires_at);
}));
$('sync').addEventListener('click',()=>action($('sync'),async()=>{
  const result=await invoke('sync_memories');await refresh();message(`Git sync complete: ${result.uploaded} events uploaded, ${result.downloaded} downloaded.`);
}));
$('prompt-form').addEventListener('submit', event => {
  event.preventDefault();
  const text = $('prompt').value.trim();
  if (!text || chatBusy) return;
  chatBusy = true;
  document.querySelectorAll('.conversation-action').forEach(b=>b.disabled=true);
  const mode = $('answer-mode').value, web = $('web-mode').value;
  $('answer-mode').disabled = true; $('web-mode').disabled = true;$('prompt').readOnly=true;
  renderConversation([...conversationState.turns,{role:'user',content:text}]);
  $('prompt').value='';resizePrompt();
  taskState={running:true,steps:[],pending:null};taskExpanded=false;$('task-summary').setAttribute('aria-expanded','false');$('task-steps').hidden=true;$('task-panel').hidden=false;$('working-indicator').hidden=false;$('task-summary').textContent='Getting started…';$('task-steps').replaceChildren();$('task-review').hidden=true;
  clearInterval(taskPoll);taskPoll=setInterval(()=>pollTask().catch(()=>{}),700);
  $('generation-status').hidden=true;
  $('generation-status').textContent = 'Choosing the model, retrieving context and preparing your answer…';
  action(event.submitter, async () => {
    try {
      const result = await invoke('send_prompt', { text, mode, web });
      conversationDrafts.delete(conversationState.id);
      $('prompt').value = '';
      resizePrompt();
      await loadConversation();
      $('generation-status').hidden=!result.incomplete;
      $('generation-status').textContent=result.incomplete?'Reply reached its output limit. Continue to finish.':'';
      await refresh();
    } catch (error) {
      $('prompt').value=text;resizePrompt();
      $('generation-status').hidden=false;
      $('generation-status').textContent = 'Your request is kept in the composer. Add details or edit it to continue.';
      renderConversation([...conversationState.turns,{role:'user',content:text},{role:'assistant',content:`The app could not receive or save the task result.\n\nDetail: ${String(error?.message??error)}\n\nTell me what you see now, or check Settings. Actions may already have completed; check their state before retrying.`,failure:{settings:true,local:false},original_prompt:text}]);
    } finally {clearInterval(taskPoll);await pollTask().catch(()=>{});$('prompt').readOnly=false; chatBusy = false;document.querySelectorAll('.conversation-action').forEach(b=>b.disabled=false); $('answer-mode').disabled = false; $('web-mode').disabled = false; }
  });
});
setInterval(()=>refresh().catch(()=>{$('beacon-warning').hidden=false;}),10000);

loadConversation().catch(error => message(String(error?.message ?? error), true));

refresh().catch(error => {
  $('connection').textContent = 'Engine unavailable';
  $('memories').replaceChildren(empty('Could not connect', 'Check the message above and try Refresh.'));
  message(String(error?.message ?? error), true);
});

const palettes={
  Paper:['#f0efe9','#dfdfd9','#3e3e40','#1c1c1e','#4d4d51','#2b2b2b','#42423e','#d6d6d2','#f0f0f0','#c4c4c2'],
  Sky:['#d9ecff','#c7def5','#2d5b84','#17384f','#29475d','#24323d','#31414f','#a5cde9','#e1edf7','#c1d4e2'],
  Lemon:['#fff6cc','#ebe2b6','#7d6625','#4e4210','#5d4b1f','#363126','#464030','#dac781','#f5edcf','#ddd2ac'],
  Mint:['#ddf4e5','#c8e1d1','#2f6a4f','#1f4c38','#315540','#25372c','#33483b','#9acfb0','#e1f1e6','#c1d8ca'],
  Rose:['#ffe1dc','#eccbc6','#8b4d47','#5f3431','#703f3c','#3d2d2b','#503c39','#e4b0aa','#f7e6e2','#e2c8c3'],
  Stone:['#dedede','#c9c9c9','#4b4b4b','#2f2f2f','#424242','#2c2c2c','#3b3b3b','#d0d0d0','#f0f0f0','#c9c9c9']
};
function applyPaper(name){const p=palettes[name]||palettes.Paper;['--paper','--secondary','--accent','--ink','--muted'].forEach((key,i)=>document.documentElement.style.setProperty(key,`light-dark(${p[i]},${p[i+5]})`));$('paper-theme').value=name in palettes?name:'Paper';$('paper-theme').dispatchEvent(new Event('picker-update'));}
try{applyPaper(localStorage.getItem('fabric-paper')||'Paper');}catch{applyPaper('Paper');}
$('paper-theme').addEventListener('change',()=>{applyPaper($('paper-theme').value);try{localStorage.setItem('fabric-paper',$('paper-theme').value);}catch{}});
function selectView(view){closePaperMenu();document.querySelector('.app').classList.toggle('settings-view',view!=='chat');$('active-view-title').textContent=({chat:conversationState.title||'Your conversation',memory:'Shared memory',connections:'Connections',devices:'Devices & sync',activity:'Activity',settings:'Settings'})[view]||view;document.querySelectorAll('[data-panel]').forEach(p=>p.hidden=p.dataset.panel!==view);document.querySelectorAll('[data-view]').forEach(b=>{if(b.dataset.view===view)b.setAttribute('aria-current','page');else b.removeAttribute('aria-current');});if(view==='activity')loadActivity().catch(e=>message(e.message,true));if(view==='connections')loadConnections().catch(e=>message(e.message,true));if(view==='settings'){loadAgentSettings().catch(e=>message(e.message,true));loadModelStatus().catch(e=>message(e.message,true));loadCloudSettings().catch(e=>message(e.message,true));loadWorkspace().catch(e=>message(e.message,true));}if(view==='devices')loadBeacon().catch(e=>message(e.message,true));}
document.querySelectorAll('[data-view]').forEach(b=>b.addEventListener('click',()=>selectView(b.dataset.view)));
$('focus-prompt').classList.add('conversation-action');
$('focus-prompt').addEventListener('click',newConversation);
$('new-conversation').addEventListener('click',newConversation);
let qrObjectURL,expiryTimer;
async function showQR(text,expires){
  const image=await invoke('qr_code',{text});
  if(qrObjectURL)URL.revokeObjectURL(qrObjectURL);qrObjectURL=URL.createObjectURL(new Blob([image],{type:'image/svg+xml'}));
  $('pairing-qr').src=qrObjectURL;$('pairing-link').value=text;$('pairing-output').hidden=false;
  $('pairing-description').textContent=expires?'Scan to pair · One use':'Beacon address · Invitation required to join';
  clearInterval(expiryTimer);
  function tick(){const seconds=expires?Math.max(0,Math.ceil(expires-Date.now()/1000)):null;$('pairing-expiry').textContent=seconds===null?'Address QR · No credentials included':seconds?`Invitation expires in ${Math.floor(seconds/60)}:${String(seconds%60).padStart(2,'0')}`:'Invitation expired. Create a new pairing QR.';if(seconds===0){$('pairing-qr').hidden=true;$('pairing-link').value='';clearInterval(expiryTimer);}else $('pairing-qr').hidden=false;}
  tick();if(expires)expiryTimer=setInterval(tick,1000);
}
async function loadBeacon(){
  const state=await invoke('beacon_settings');
  $('beacon-url').value=state.url;$('device-name').value=state.name||'This Mac';
  $('this-device-name').textContent=state.name||'This Mac';$('beacon-address').textContent=state.url;
  $('beacon-state').textContent=state.connected?(beaconOnline?'Shared memory is up to date.':'Waiting to reconnect.'):'Connect a beacon to sync your memory.';
  $('disconnect-beacon').hidden=!state.connected;await loadLocalBeacon();return state;
}
$('configure-beacon').addEventListener('click',()=>loadBeacon().then(()=>$('beacon-settings').showModal()).catch(e=>message(e.message,true)));
$('close-beacon-settings').addEventListener('click',()=>$('beacon-settings').close());
$('beacon-form').addEventListener('submit',e=>{e.preventDefault();action(e.submitter,async()=>{
  const url=$('beacon-url').value.trim();let display=url;
  if(url.startsWith('fabric-memory://')){const parsed=new URL(url);display=parsed.searchParams.get('url')||url;}else{try{const parsed=new URL(url);parsed.hash='';display=parsed.href;}catch{}}
  if(!await confirmJoin(display))return;
  await invoke('beacon_connect',{url,code:$('beacon-code').value.trim(),name:$('device-name').value.trim()});$('beacon-code').value='';$('beacon-settings').close();await loadBeacon();await refresh();message('Beacon paired. Automatic sync is enabled.');
});});
$('save-beacon-url').addEventListener('click',()=>action($('save-beacon-url'),async()=>{await invoke('beacon_save_url',{url:$('beacon-url').value.trim()});await loadBeacon();$('beacon-settings').close();message('Beacon URL saved.');}));
$('disconnect-beacon').addEventListener('click',()=>action($('disconnect-beacon'),async()=>{await invoke('beacon_disconnect');$('pairing-output').hidden=true;clearInterval(expiryTimer);await loadBeacon();await refresh();message('This device is disconnected. Its local memories are retained.');}));
$('show-address-qr').addEventListener('click',()=>action($('show-address-qr'),async()=>{const raw=$('beacon-url').value.trim();let url;try{url=new URL(raw);}catch{throw new Error('Enter a complete HTTPS beacon URL.');}if(url.protocol!=='https:'||url.username||url.password||url.search||url.hash)throw new Error('Use an HTTPS beacon address without credentials or query parameters.');await showQR('fabric-memory://pair?url='+encodeURIComponent(url.href.replace(/\/$/,'')));}));
$('copy-pairing').addEventListener('click',()=>navigator.clipboard.writeText($('pairing-link').value).then(()=>message('Pairing link copied.')).catch(()=>{$('pairing-link').select();message('Select and copy the pairing link.');}));
loadBeacon().catch(()=>{});
function confirmJoin(address){const dialog=$('confirm-join');$('confirm-beacon-address').textContent=address;dialog.returnValue='cancel';return new Promise(resolve=>{dialog.addEventListener('close',()=>resolve(dialog.returnValue==='join'),{once:true});dialog.showModal();});}

// Native window chrome and paper controls.
document.querySelectorAll('[data-window]').forEach(button=>button.addEventListener('click',()=>invoke('window_'+button.dataset.window).catch(e=>message(e.message,true))));
document.querySelector('.titlebar').addEventListener('mousedown',event=>{
  if(event.button===0&&!event.target.closest('button')){event.preventDefault();invoke(event.detail===2?'window_maximize':'window_drag').catch(e=>message(e.message,true));}
});
$('cycle-paper').addEventListener('click',()=>{
  const names=Object.keys(palettes);const name=names[(names.indexOf($('paper-theme').value)+1)%names.length];applyPaper(name);
  try{localStorage.setItem('fabric-paper',name);}catch{}
});
$('toggle-context').addEventListener('click',()=>{
  const hidden=document.querySelector('.app').classList.toggle('no-context');$('toggle-context').setAttribute('aria-pressed',String(!hidden));
});
function renderContext(){
  const last=conversationState.turns.filter(turn=>turn.role==='assistant').at(-1);
  $('context-model').textContent=last?.model||conversationState.model||'Local model';
  $('context-executor').textContent=last?.provider==='cloud'?'Cloud provider':'This Mac';
  $('web-connection-mode').textContent=({auto:'Auto',on:'On',off:'Off'})[$('web-mode').value];
  $('context-web').textContent=({auto:'Auto',on:'On',off:'Off'})[$('web-mode').value];
  $('context-cloud').textContent=$('answer-mode').value==='local'?'□ Local answers only':cloudAvailable?'□ Cloud available':'□ Cloud not configured';
  const list=$('context-memories');list.replaceChildren();
  if(!allMemories.length){const p=document.createElement('p');p.className='chat-hint';p.textContent='No saved memories yet.';list.append(p);return;}
  allMemories.slice(-3).reverse().forEach(memory=>{
    const row=document.createElement('div');row.className='context-memory';
    const plain=document.createElement('div');plain.innerHTML=DOMPurify.sanitize(marked.parse(memory.text));const text=plain.textContent.trim();
    const p=document.createElement('p');p.textContent=text.length>140?text.slice(0,137)+'…':text;
    const source=document.createElement('small');source.textContent=memory.translated?'Synced · MiniLM → MPNet':'Saved on this Mac · MPNet';
    row.append(p,source);list.append(row);
  });
}
function renderActivity(turns){
  const list=$('activity-list');list.replaceChildren();
  const replies=turns.filter(turn=>turn.role==='assistant').slice(-20).reverse();
  if(!replies.length){list.append(empty('No activity yet','Your model and source receipts will appear here after a reply.'));return;}
  replies.forEach(turn=>{
    const row=document.createElement('article');row.className='activity-item';
    const title=document.createElement('strong');title.textContent=(turn.provider==='cloud'?'Cloud':'This Mac')+' · '+(turn.model||'Assistant');
    const receipt=document.createElement('p');receipt.textContent=[turn.route_reason,Array.isArray(turn.sources)?turn.sources.length+' web sources':null].filter(Boolean).join(' · ');
    row.append(title,receipt);list.append(row);
  });
}

const icons={settings:'<path d="M12 3v3m0 12v3M3 12h3m12 0h3M5.6 5.6l2.1 2.1m8.6 8.6 2.1 2.1m0-12.8-2.1 2.1m-8.6 8.6-2.1 2.1"/><circle cx="12" cy="12" r="5"/>',chat:'<path d="M21 11.5a8.4 8.4 0 0 1-.9 3.8 8.5 8.5 0 0 1-7.6 4.7 8.4 8.4 0 0 1-3.8-.9L3 21l1.9-5.7a8.4 8.4 0 0 1-.9-3.8 8.5 8.5 0 0 1 4.7-7.6 8.4 8.4 0 0 1 3.8-.9h.5a8.5 8.5 0 0 1 8 8z"/>',memory:'<path d="M4 19.5A2.5 2.5 0 0 1 6.5 17H20M6.5 3H20v19H6.5A2.5 2.5 0 0 1 4 19.5v-14A2.5 2.5 0 0 1 6.5 3z"/>',connections:'<rect x="3" y="3" width="7" height="7"/><rect x="14" y="3" width="7" height="7"/><rect x="3" y="14" width="7" height="7"/><rect x="14" y="14" width="7" height="7"/>',devices:'<rect x="2" y="3" width="16" height="12" rx="1"/><path d="M7 21h6m-3-6v6"/><rect x="17" y="10" width="5" height="11" rx="1"/>',activity:'<path d="M3 12h4l3-8 4 16 3-8h4"/>'};
Object.assign(icons,{
  laptop:'<rect x="3" y="3" width="18" height="13" rx="1"/><path d="M1 21h22l-2-5H3z"/>',
  server:'<rect x="3" y="3" width="18" height="7" rx="1"/><rect x="3" y="14" width="18" height="7" rx="1"/><path d="M7 6.5h.01M7 17.5h.01"/>',
  phone:'<rect x="6" y="2" width="12" height="20" rx="2"/><path d="M11 18h2"/>',
  cpu:'<rect x="5" y="5" width="14" height="14" rx="1"/><rect x="9" y="9" width="6" height="6"/><path d="M9 1v4m6-4v4M9 19v4m6-4v4M1 9h4m-4 6h4m14-6h4m-4 6h4"/>',
  cloud:'<path d="M20 16.5a4.5 4.5 0 0 0-1-8.8A7 7 0 0 0 5.3 6 5.5 5.5 0 0 0 6 17h13"/>',
  globe:'<circle cx="12" cy="12" r="9"/><path d="M3 12h18M12 3a16 16 0 0 1 0 18 16 16 0 0 1 0-18Z"/>'
});
for(const element of document.querySelectorAll('[data-icon]')){
  element.innerHTML='<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">'+icons[element.dataset.icon]+'</svg>';
}
document.querySelectorAll('.sidebar nav [data-view]').forEach(button=>{
  const svg=document.createElementNS('http://www.w3.org/2000/svg','svg');svg.setAttribute('viewBox','0 0 24 24');svg.setAttribute('fill','none');svg.setAttribute('stroke','currentColor');svg.setAttribute('stroke-width','1.5');svg.setAttribute('stroke-linecap','round');svg.setAttribute('stroke-linejoin','round');svg.setAttribute('aria-hidden','true');svg.innerHTML=icons[button.dataset.view];button.prepend(svg);
});

function resizePrompt(){
  const prompt=$('prompt');
  prompt.style.height='auto';
  const height=Math.min(120,Math.max(40,prompt.scrollHeight));
  prompt.style.height=height+'px';
  prompt.style.overflowY=prompt.scrollHeight>120?'auto':'hidden';
}
$('prompt').addEventListener('input',()=>{conversationDrafts.set(conversationState.id,$('prompt').value);resizePrompt();});
new ResizeObserver(()=>resizePrompt()).observe(document.querySelector('.content-area'));
requestAnimationFrame(resizePrompt);

// Paper menus retain the select values used by routing and palette preferences.

for(const select of document.querySelectorAll('select')){
  const wrapper=document.createElement('div');wrapper.className='paper-select';select.before(wrapper);wrapper.append(select);
  select.hidden=true;select.tabIndex=-1;
  const trigger=document.createElement('button');trigger.type='button';trigger.className='paper-select-trigger';trigger.id=select.id+'-picker';
  const label=document.querySelector('label[for="'+select.id+'"]');
  trigger.setAttribute('aria-label',select.getAttribute('aria-label')||label?.textContent||select.id);
  if(label)label.htmlFor=trigger.id;
  trigger.setAttribute('aria-haspopup','listbox');trigger.setAttribute('aria-expanded','false');
  const menu=document.createElement('div');menu.className='paper-select-menu';menu.id=select.id+'-options';menu.setAttribute('role','listbox');menu.setAttribute('aria-label',trigger.getAttribute('aria-label'));menu.hidden=true;trigger.setAttribute('aria-controls',menu.id);
  let choices=[];
  function rebuild(){menu.replaceChildren();choices=Array.from(select.options).map(option=>{
    const button=document.createElement('button');button.type='button';button.setAttribute('role','option');button.tabIndex=-1;button.textContent=option.textContent;button.dataset.value=option.value;button.disabled=option.disabled;
    button.addEventListener('click',()=>choose(button));menu.append(button);return button;
  });update();}
  function update(){trigger.textContent=select.selectedOptions[0]?.textContent||'';trigger.disabled=select.disabled;choices.forEach(button=>button.setAttribute('aria-selected',String(button.dataset.value===select.value)));}
  function close(restore=false){menu.hidden=true;trigger.setAttribute('aria-expanded','false');if(restore)trigger.focus();}
  function choose(button){if(button.disabled)return;select.value=button.dataset.value;select.dispatchEvent(new Event('change',{bubbles:true}));update();close(true);}
  function open(index=select.selectedIndex){
    closePaperMenu();update();if(select.disabled)return;menu.hidden=false;trigger.setAttribute('aria-expanded','true');
    const rect=trigger.getBoundingClientRect();menu.style.minWidth=Math.max(110,rect.width)+'px';
    const height=menu.getBoundingClientRect().height;menu.style.left=Math.min(rect.left,window.innerWidth-menu.getBoundingClientRect().width-8)+'px';menu.style.top=(rect.bottom+height+6>window.innerHeight?Math.max(8,rect.top-height-4):rect.bottom+4)+'px';
    choices[Math.max(0,index)]?.focus();closePaperMenu=()=>close();
  }
  trigger.addEventListener('click',()=>menu.hidden?open():close());
  trigger.addEventListener('keydown',event=>{if(['ArrowDown','ArrowUp'].includes(event.key)){event.preventDefault();open();}});
  menu.addEventListener('keydown',event=>{
    const index=choices.indexOf(document.activeElement);
    if(event.key==='Escape'){event.preventDefault();close(true);}
    else if(event.key==='Tab'){close();trigger.focus();}
    else if(['ArrowDown','ArrowUp','Home','End'].includes(event.key)){
      event.preventDefault();const next=event.key==='Home'?0:event.key==='End'?choices.length-1:(index+(event.key==='ArrowDown'?1:-1)+choices.length)%choices.length;choices[next].focus();
    }
  });
  document.addEventListener('pointerdown',event=>{if(!wrapper.contains(event.target))close();});
  window.addEventListener('resize',()=>close());
  document.addEventListener('scroll',event=>{if(!menu.contains(event.target))close();},true);
  select.addEventListener('change',update);select.addEventListener('picker-update',rebuild);
  new MutationObserver(rebuild).observe(select,{attributes:true,childList:true,subtree:true,characterData:true,attributeFilter:['disabled','selected','label','value']});
  wrapper.append(trigger,menu);rebuild();
}

async function switchConversation(id){
  if(chatBusy||switchingConversation)return;
  switchingConversation=true;
  conversationDrafts.set(conversationState.id,$('prompt').value);
  try{await invoke('conversation_select',{id});await loadConversation();selectView('chat');}catch(e){message(e.message,true);}finally{switchingConversation=false;}
}
async function newConversation(){
  if(chatBusy||switchingConversation)return;
  switchingConversation=true;
  conversationDrafts.set(conversationState.id,$('prompt').value);
  try{await invoke('conversation_create');await loadConversation();selectView('chat');$('prompt').focus();}catch(e){message(e.message,true);}finally{switchingConversation=false;}
}
function renderConversationList(items){
  const list=$('conversation-list');list.replaceChildren();
  items.forEach(chat=>{
    const button=document.createElement('button');button.className='recent-conversation conversation-action';button.disabled=chatBusy;button.textContent=chat.title;button.title=chat.title;button.dataset.conversationId=chat.id;
    if(chat.active)button.setAttribute('aria-current','page');
    const subtitle=document.createElement('small');subtitle.textContent=chat.messages?chat.messages+' messages':'New · This Mac';button.append(subtitle);button.addEventListener('click',()=>switchConversation(chat.id));list.append(button);
  });
}

async function deleteConversation(id){
  if(chatBusy)return;
  try{await invoke('conversation_delete',{id});conversationDrafts.delete(id);await loadConversation();message('Conversation moved to Recently deleted.');}catch(e){message(e.message,true);}
}
function renderDeletedConversations(){
  const list=$('deleted-list');list.replaceChildren();
  for(const chat of conversationState.deleted||[]){const row=document.createElement('div');row.className='service-row';const title=document.createElement('span');title.className='service-copy';title.textContent=chat.title;const restore=document.createElement('button');restore.className='secondary';restore.textContent='Restore';restore.addEventListener('click',()=>action(restore,async()=>{await invoke('conversation_restore',{id:chat.id});await loadConversation();selectView('chat');if(!conversationState.deleted.length)$('deleted-dialog').close();}));row.append(title,restore);list.append(row);}
}
$('deleted-conversations').addEventListener('click',()=>$('deleted-dialog').showModal());$('close-deleted').addEventListener('click',()=>$('deleted-dialog').close());
function hideContextMenu(){$('context-menu').hidden=true;}
function contextMenu(event,items){
  event.preventDefault();closePaperMenu();const menu=$('context-menu');menu.replaceChildren();
  for(const item of items){const b=document.createElement('button');b.type='button';b.setAttribute('role','menuitem');b.textContent=item.label;b.disabled=!!item.disabled;if(item.danger)b.className='danger';b.addEventListener('click',()=>{hideContextMenu();Promise.resolve(item.run()).catch(e=>message(e.message,true));});menu.append(b);}
  menu.hidden=false;const rect=menu.getBoundingClientRect();menu.style.left=Math.max(4,Math.min(event.clientX,innerWidth-rect.width-4))+'px';menu.style.top=Math.max(4,Math.min(event.clientY,innerHeight-rect.height-4))+'px';menu.firstElementChild?.focus();
}
document.addEventListener('contextmenu',event=>{
  const target=event.target;
  const chat=target.closest('[data-conversation-id]');
  if(chat)return contextMenu(event,[{label:'Open conversation',disabled:chatBusy,run:()=>switchConversation(chat.dataset.conversationId)},{label:'Rename conversation',disabled:chatBusy,run:()=>renameConversation(chat.dataset.conversationId)},{label:'Delete conversation',danger:true,disabled:chatBusy,run:()=>deleteConversation(chat.dataset.conversationId)}]);
  const input=target.closest('textarea,input');
  if(input){
    if(input.disabled){event.preventDefault();hideContextMenu();return;}
    const start=input.selectionStart??0,end=input.selectionEnd??input.value.length,selection=input.value.slice(start,end);
    const write=text=>{input.focus();if(input.selectionStart!==null)input.setRangeText(text,start,end,'end');else input.value=text;input.dispatchEvent(new Event('input',{bubbles:true}));};
    const items=[{label:'Copy',disabled:!selection,run:()=>invoke('clipboard_write',{text:selection})}];
    if(!input.readOnly){items.unshift({label:'Cut',disabled:!selection,run:async()=>{await invoke('clipboard_write',{text:selection});write('');}});items.push({label:'Paste',run:async()=>write(await invoke('clipboard_read'))});}
    items.push({label:'Select all',run:()=>{input.focus();input.select();}});return contextMenu(event,items);
  }
  const selection=window.getSelection()?.toString()||'';
  const items=selection?[{label:'Copy selection',run:()=>invoke('clipboard_write',{text:selection})}]:[];
  const link=target.closest('a[href]');
  if(link){items.push({label:'Open link',run:()=>invoke('open_web_source',{url:link.href})},{label:'Copy link',run:()=>invoke('clipboard_write',{text:link.href})});return contextMenu(event,items);}
  const memory=target.closest('[data-memory-id]');
  if(memory){items.push({label:'Copy memory',run:()=>invoke('clipboard_write',{text:memory.querySelector('.memory-text').textContent})},{label:'Forget memory',danger:true,disabled:chatBusy,run:async()=>{await invoke('forget',{memory:memory.dataset.memoryId});await refresh();if(searchActive)await performSearch();}});return contextMenu(event,items);}
  const turn=target.closest('.chat-turn');
  if(turn){items.push({label:turn.dataset.role==='user'?'Copy message':'Copy reply',run:()=>invoke('clipboard_write',{text:turn.dataset.messageText})});if(turn.dataset.role==='user')items.push({label:'Use as prompt',disabled:chatBusy,run:()=>{$('prompt').value=turn.dataset.messageText;resizePrompt();$('prompt').focus();}});return contextMenu(event,items);}
  if(!items.length && target.closest('[data-panel=memory]'))items.push({label:'Add memory',run:()=>$('memory-text').focus()},{label:'Clear memories…',danger:true,disabled:chatBusy||!totalMemories,run:openClearMemories});
  else if(!items.length && (target.closest('.sidebar')||target.closest('[data-panel=chat]')))items.push({label:'New conversation',disabled:chatBusy,run:newConversation});
  if(items.length)return contextMenu(event,items);
  event.preventDefault();hideContextMenu();
});
let clearMemoryCount=0;
async function openClearMemories(){if(chatBusy)return;await refresh();clearMemoryCount=totalMemories;if(!clearMemoryCount)return message('There are no saved memories to clear.');$('clear-memory-title').textContent=`Clear ${clearMemoryCount} saved memories?`;$('clear-memory-dialog').showModal();$('cancel-clear-memory').focus();}
$('clear-memories').addEventListener('click',()=>openClearMemories().catch(e=>message(e.message,true)));
$('cancel-clear-memory').addEventListener('click',()=>$('clear-memory-dialog').close());
$('clear-memory-form').addEventListener('submit',event=>{event.preventDefault();action($('confirm-clear-memory'),async()=>{const result=await invoke('clear_memories',{confirmation:'clear-saved-memories',expected_count:clearMemoryCount});$('clear-memory-dialog').close();searchActive=false;$('query').value='';await refresh();message(`${result.cleared} saved memories cleared. Conversations were kept.`);});});

document.addEventListener('pointerdown',event=>{if(!$('context-menu').contains(event.target))hideContextMenu();});
window.addEventListener('resize',hideContextMenu);document.addEventListener('scroll',hideContextMenu,true);
$('context-menu').addEventListener('keydown',event=>{const items=Array.from($('context-menu').querySelectorAll('button:not(:disabled)'));const index=items.indexOf(document.activeElement);if(event.key==='Escape'){hideContextMenu();}else if(['ArrowDown','ArrowUp'].includes(event.key)){event.preventDefault();items[(index+(event.key==='ArrowDown'?1:-1)+items.length)%items.length]?.focus();}});

let modelPoll;
async function loadModelStatus(){const state=await invoke('model_status');$('configured-model').textContent=state.model;$('load-model').disabled=state.loading;$('model-request').disabled=state.loading;$('model-progress').textContent=state.loading?`${state.status} · ${Math.round(state.progress||0)}%`:state.status==='Ready'?'':state.status;$('model-download').hidden=!state.loading;$('model-download').value=state.progress||0;if(!state.loading&&modelPoll){clearInterval(modelPoll);modelPoll=null;await loadConversation();}return state;}
$('model-form').addEventListener('submit',async event=>{event.preventDefault();$('load-model').disabled=true;try{await invoke('model_load',{prompt:$('model-request').value});await loadModelStatus();if(!modelPoll)modelPoll=setInterval(()=>loadModelStatus().catch(e=>message(e.message,true)),1500);}catch(e){message(e.message,true);await loadModelStatus();}});

async function pollTask(){
  const state=await invoke('agent_status');taskState=state;
  $('task-panel').hidden=!state.running&&!state.pending&&!(taskExpanded&&state.steps?.length);$('stop-task').hidden=!state.running;$('stop-task').disabled=state.stopped;$('stop-task').textContent=state.stopped?'Stopping…':'Stop';
  $('working-indicator').hidden=!state.running||!!state.pending;const current=state.steps?.at(-1);$('task-summary').textContent=state.stopped?(state.running?'Stopping…':'Stopped'):!state.running?(state.outcome===false?'Task ended · view activity':'Task complete · view activity'):state.pending?state.pending.summary:current?.kind==='planning'?(current.summary||'Choosing next action…'):current?.kind==='done'?'Checking results…':current?.summary||'Getting started…';
  renderTaskActivity(state);

  $('task-review').hidden=!state.pending;if(state.pending){$('review-summary').textContent=state.pending.tool;$('review-arguments').textContent=JSON.stringify(state.pending.arguments,null,2);}
}
$('approve-action').addEventListener('click',()=>reviewAction(true));$('deny-action').addEventListener('click',()=>reviewAction(false));
async function reviewAction(allow){if(!taskState.pending)return;const id=taskState.pending.id;try{await invoke('agent_review',{id,allow});await pollTask();}catch(e){message(e.message,true);}}
$('stop-task').addEventListener('click',()=>invoke('agent_stop').then(pollTask).catch(e=>message(e.message,true)));
async function loadConnections(){
  const computer=await invoke('computer_status');computerEnabled=computer.enabled;$('toggle-computer').textContent=computer.enabled?'Disable':'Enable';$('computer-state').textContent=!computer.enabled?'Apps, keyboard, and screen':computer.accessibility?'Accessibility ready':'Needs Accessibility access';$('computer-permissions').hidden=!computer.enabled;
  $('mcp-status').textContent='Checking connections…';const servers=await invoke('mcp_catalog');$('mcp-status').textContent=servers.length?servers.filter(s=>s.status==='Connected').length+' connected · '+servers.length+' configured':'No servers configured';const list=$('mcp-servers');list.replaceChildren();
  for(const server of servers){const row=document.createElement('div');row.className='service-row mcp-server-row';const copy=document.createElement('div');copy.className='service-copy';const title=document.createElement('h2');title.textContent=server.name;const state=document.createElement('p');state.textContent=server.status==='Connected'?server.tools.length+' tools · '+server.resources.length+' resources · '+server.prompts.length+' prompts':server.status;copy.append(title,state);const browse=document.createElement('button');browse.className='text-button';browse.textContent='Browse';browse.disabled=server.status!=='Connected';browse.addEventListener('click',()=>browseMCP(server));const restart=document.createElement('button');restart.className='text-button';restart.textContent='Reconnect';restart.addEventListener('click',()=>action(restart,async()=>{await invoke('mcp_restart',{server:server.name});await loadConnections();}));row.append(copy,browse,restart);list.append(row);}
}
$('toggle-computer').addEventListener('click',async()=>{$('toggle-computer').disabled=true;try{await invoke('computer_enable',{enabled:!computerEnabled});}catch(e){message(e.message,true);}finally{$('toggle-computer').disabled=false;await loadConnections();}});
$('computer-permissions').addEventListener('click',()=>action($('computer-permissions'),async()=>{await invoke('computer_permissions');await loadConnections();}));
$('configure-mcp').addEventListener('click',()=>action($('configure-mcp'),async()=>{const config=await invoke('mcp_config');$('mcp-config-json').value=JSON.stringify(config,null,2);$('mcp-config-dialog').showModal();}));
$('close-mcp-config').addEventListener('click',()=>$('mcp-config-dialog').close());$('close-mcp-tools').addEventListener('click',()=>$('mcp-tools-dialog').close());
$('mcp-config-form').addEventListener('submit',event=>{event.preventDefault();action(event.submitter,async()=>{const config=JSON.parse($('mcp-config-json').value);await invoke('mcp_save',{config});$('mcp-config-dialog').close();await loadConnections();});});
let mcpCall;
function browseMCP(server){
  $('mcp-server-title').textContent=server.name;$('mcp-call-form').hidden=true;$('mcp-result').hidden=true;const list=$('mcp-capabilities');list.replaceChildren();
  for(const [group,method] of [['tools','tools/call'],['resources','resources/read'],['prompts','prompts/get']]){if(!server[group].length)continue;const heading=document.createElement('p');heading.className='eyebrow';heading.textContent=group.toUpperCase();list.append(heading);for(const item of server[group]){const button=document.createElement('button');button.className='capability-button';button.textContent=item.title||item.name||item.uri;button.addEventListener('click',()=>{mcpCall={server:server.name,method,item};$('mcp-call-title').textContent=button.textContent;$('mcp-call-arguments').value=method==='resources/read'?JSON.stringify({uri:item.uri},null,2):'{}';$('mcp-call-arguments').placeholder=JSON.stringify(item.inputSchema||{},null,2);$('mcp-call-form').hidden=false;$('mcp-result').hidden=true;});list.append(button);}}
  $('mcp-tools-dialog').showModal();
}
$('mcp-call-form').addEventListener('submit',event=>{event.preventDefault();action(event.submitter,async()=>{const args=JSON.parse($('mcp-call-arguments').value);const params=mcpCall.method==='tools/call'?{name:mcpCall.item.name,arguments:args}:mcpCall.method==='prompts/get'?{name:mcpCall.item.name,arguments:args}:args;try{const result=await invoke('mcp_call',{server:mcpCall.server,method:mcpCall.method,params});$('mcp-result').textContent=JSON.stringify(result,null,2);}catch(e){$('mcp-result').textContent=e.message;}$('mcp-result').hidden=false;});});

async function loadActivity(){
 const history=await invoke('agent_history');if(!history.length){renderActivity(conversationState.turns);return;}
 const list=$('activity-list');list.replaceChildren();
 for(const entry of history.filter(e=>!e.phase&&!['planning','plan','answering'].includes(e.kind))){const row=document.createElement('article');row.className='activity-item';const title=document.createElement('strong');title.textContent=(entry.kind==='done'?'✓ ':entry.kind==='error'?'× ':'· ')+entry.summary;const meta=document.createElement('p');meta.textContent=[new Date(entry.time*1000).toLocaleTimeString([],{hour:'2-digit',minute:'2-digit'}),entry.tool||'This Mac'].join(' · ');row.append(title,meta);if(entry.result!==undefined){const details=document.createElement('details');const summary=document.createElement('summary');summary.textContent='Output';const pre=document.createElement('pre');pre.className='tool-result';const value=entry.result;pre.textContent=typeof value==='string'?value:(typeof value?.stdout==='string'||typeof value?.stderr==='string')?[value.stdout,value.stderr].filter(Boolean).join('\n'):JSON.stringify(value,null,2);details.append(summary,window.FabricChat.copyButton(pre.textContent,'Copy output'),pre);row.append(details);}list.append(row);}
}

let renameTarget;
function renameConversation(id){if(chatBusy)return;const chat=(conversationState.conversations||[]).find(c=>c.id===id);if(!chat)return;renameTarget=id;$('conversation-name').value=chat.title;$('rename-dialog').showModal();$('conversation-name').select();}
$('active-view-title').addEventListener('dblclick',()=>{if(!document.querySelector('[data-panel=chat]').hidden)renameConversation(conversationState.id);});
$('rename-form').addEventListener('submit',event=>{event.preventDefault();action(event.submitter,async()=>{await invoke('conversation_rename',{id:renameTarget,title:$('conversation-name').value});$('rename-dialog').close();await loadConversation();});});
$('cancel-rename').addEventListener('click',()=>$('rename-dialog').close());

let cloudCatalogRequest=0;
function fillCloudModels(models,pinned){const select=$('cloud-model');select.replaceChildren();const ids=[...new Set([pinned,...models].filter(Boolean))];for(const id of ids){const option=document.createElement('option');option.value=id;option.textContent=id+(models.length&&!models.includes(id)?' · unavailable':models.includes(id)?'':' · saved');option.disabled=!!models.length&&!models.includes(id);select.append(option);}select.value=models.length&&!models.includes(pinned)?models[0]:pinned||ids[0]||'';}
async function loadCloudCatalog(refresh=false){const request=++cloudCatalogRequest;const state=await invoke('cloud_models',{refresh});if(request!==cloudCatalogRequest)return;fillCloudModels(state.models,$('cloud-model').value);$('cloud-model-pool').textContent=`${state.models.length} available models${state.stale?' · saved catalog; refresh to update':''}`;}
async function loadCloudSettings(){cloudCatalogRequest++;const state=await invoke('cloud_settings');const providers=$('cloud-provider');providers.replaceChildren();for(const p of state.providers){const option=document.createElement('option');option.value=p.id;option.textContent=p.name;providers.append(option);}providers.value=state.provider_id;$('cloud-provider-endpoint').textContent=state.base_url;fillCloudModels([],state.model);$('cloud-automatic').checked=state.automatic;$('cloud-model').disabled=state.automatic;$('cloud-model-pool').textContent=state.last_selected?`Last selected: ${state.last_selected}`:'Save a token to discover available models.';$('cloud-enabled').checked=state.enabled;renderCloudCredential(state);loadAgentSettings().catch(()=>{});if(state.token_configured)loadCloudCatalog().catch(e=>{$('cloud-model-pool').textContent=e.message+' · saved model retained';});return state;}
function settingsTab(tab){for(const id of ['general','cloud']){$('settings-'+id).hidden=id!==tab;$('settings-'+id+'-tab').setAttribute('aria-selected',String(id===tab));}}
for(const tab of ['general','cloud'])$('settings-'+tab+'-tab').addEventListener('click',()=>settingsTab(tab));
$('add-cloud-provider').addEventListener('click',()=>{$('provider-form').hidden=false;$('provider-name').focus();});
$('cancel-provider').addEventListener('click',()=>{$('provider-form').hidden=true;});
$('provider-form').addEventListener('submit',event=>{event.preventDefault();action(event.submitter,async()=>{await invoke('cloud_provider_add',{name:$('provider-name').value,base_url:$('provider-url').value});$('provider-form').reset();$('provider-form').hidden=true;$('cloud-token').value='';await loadCloudSettings();await loadConversation();message('Provider added. Add its token to connect.');});});
$('cloud-provider').addEventListener('change',()=>action($('cloud-provider'),async()=>{await invoke('cloud_provider_select',{id:$('cloud-provider').value});$('cloud-token').value='';await loadCloudSettings();await loadWorkspace();await loadConversation();}));
$('cloud-form').addEventListener('submit',event=>{event.preventDefault();action(event.submitter,async()=>{await invoke('cloud_save',{model:$('cloud-model').value,token:$('cloud-token').value,enabled:$('cloud-enabled').checked,automatic:$('cloud-automatic').checked});$('cloud-token').value='';await loadCloudSettings();await loadConversation();message('Cloud settings saved.');});});
$('remove-cloud-token').addEventListener('click',()=>action($('remove-cloud-token'),async()=>{await invoke('cloud_remove_token');$('cloud-token').value='';await loadCloudSettings();await loadConversation();message('Saved token removed and cloud answers disabled.');}));

async function loadLocalBeacon(){const state=await invoke('local_beacon_status');$('local-beacon-state').textContent=state.running?`Running · ${state.url} · While this app is open`:state.error||'Host on this Mac while the app is open.';$('deploy-local-beacon').hidden=state.running;$('stop-local-beacon').hidden=!state.running;return state;}
$('deploy-local-beacon').addEventListener('click',()=>action($('deploy-local-beacon'),async()=>{const previous=await invoke('beacon_settings');if(previous.connected&&!previous.local_owned&&!await confirmJoin('Switch this Mac to a new local memory beacon?'))return;$('local-beacon-state').textContent='Starting local beacon…';await invoke('local_beacon_deploy');await loadBeacon();await refresh();message('Local beacon deployed and connected. Use Add device to pair your phone on the same network.');}));
$('stop-local-beacon').addEventListener('click',()=>action($('stop-local-beacon'),async()=>{await invoke('local_beacon_stop');await loadLocalBeacon();await refresh();message('Local beacon stopped. Saved memories are retained.');}));

$('cloud-automatic').addEventListener('change',()=>{$('cloud-model').disabled=$('cloud-automatic').checked;});
$('refresh-cloud-models').addEventListener('click',()=>action($('refresh-cloud-models'),()=>loadCloudCatalog(true)));
async function loadWorkspace(){const state=await invoke('workspace_settings');$('workspace-path').value=state.path;$('workspace-cloud').checked=state.cloud_allowed;}
$('workspace-form').addEventListener('submit',event=>{event.preventDefault();action(event.submitter,async()=>{await invoke('workspace_save',{path:$('workspace-path').value.trim(),cloud_allowed:$('workspace-cloud').checked});$('workspace-status').textContent='Workspace ready. Ask the agent to implement, run, or test your code.';});});

$('retry-cloud-keychain').addEventListener('click',()=>action($('retry-cloud-keychain'),async()=>{await invoke('cloud_keychain_retry');await loadCloudSettings();await loadConversation();}));

let cloudKeychainPending=false;
function renderCloudCredential(state){cloudKeychainPending=!!state.keychain_pending;$('retry-cloud-keychain').hidden=!state.credential_error&&!state.keychain_pending;$('retry-cloud-keychain').disabled=!!state.keychain_pending;$('cloud-token-state').textContent=state.keychain_pending?'Waiting for Keychain access. Complete the macOS prompt or enter your token again.':state.credential_error|| (state.credential_source==='keychain'?'API token saved in Keychain.':state.credential_source==='environment'?'Using your private environment token.':'Add an API token for this provider.');$('remove-cloud-token').hidden=state.credential_source!=='keychain';}
setInterval(async()=>{if(!cloudKeychainPending)return;try{const state=await invoke('cloud_settings');if(state.provider_id!==$('cloud-provider').value)return;renderCloudCredential(state);if(!state.keychain_pending&&state.token_configured){cloudAvailable=state.enabled;updateRoutingHint();await loadCloudCatalog();}}catch{}},2000);

function renderTaskActivity(state){
 const list=$('task-steps');list.hidden=!taskExpanded;$('task-summary').setAttribute('aria-expanded',String(taskExpanded));list.replaceChildren();
 if(!taskExpanded)return;
 for(const step of state.steps||[]){
  if(step.background||!['running','done','error','denied','approved','planning'].includes(step.kind))continue;
  const line=document.createElement('div');line.className='step-line '+step.kind;
  const label=step.kind==='planning'?(step.summary||'Choosing next action'):step.approval==='automatic'?'Auto-approved · '+step.summary:step.summary||'Action';
  let result='';if(step.kind==='done'){const value=step.result;if(typeof value?.exit_code==='number')result=' · exit '+value.exit_code;else if(value?.window_title)result=' · '+value.window_title;else if(value?.opened)result=' · '+value.opened;}
  line.textContent=label+result;list.append(line);
 }
 if(!list.childNodes.length){const line=document.createElement('p');line.className='chat-hint';line.textContent='Preparing the task…';list.append(line);}
}
$('task-summary').addEventListener('click',()=>{taskExpanded=!taskExpanded;renderTaskActivity(taskState);$('task-panel').hidden=!taskState.running&&!taskState.pending&&!taskExpanded;});
async function loadAgentSettings(){const state=await invoke('agent_settings');$('auto-approve-actions').checked=state.auto_approve;$('cloud-computer').checked=state.computer_cloud;$('agent-approval-state').textContent=state.auto_approve?'All eligible actions run automatically.':'Actions ask for review.';$('cloud-computer-state').textContent=state.computer_cloud?`${state.provider} can plan from app observations.`:'Computer planning stays on this Mac.';return state;}
$('auto-approve-actions').addEventListener('change',async()=>{try{await invoke('agent_settings_save',{auto_approve:$('auto-approve-actions').checked});await loadAgentSettings();}catch(e){message(e.message,true);await loadAgentSettings();}});
$('cloud-computer').addEventListener('change',async()=>{try{await invoke('agent_settings_save',{computer_cloud:$('cloud-computer').checked,provider_id:$('cloud-provider').value});await loadAgentSettings();}catch(e){message(e.message,true);await loadAgentSettings();}});
