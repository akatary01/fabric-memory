const $ = id => document.getElementById(id);
let allMemories = [];
let searchActive = false;
let totalMemories = 0;
let refreshVersion = 0, searchVersion = 0;
let memorySnapshot = null;
let memoryWrite = false;
let messageTimer;
let chatBusy = false;
const activeRuns = new Map();
const queuedPrompts = new Map();
const steeringChats = new Set();
let runningTimes = {};
let conversationLoad = 0;
let taskTimeLoad = 0;
let cloudAvailable = false;
let conversationState = { turns: [] };
let beaconOnline=false;
const conversationDrafts=new Map();
const attachmentDrafts=new Map();
let closePaperMenu=()=>{};
let switchingConversation=false;
let taskState={running:false};
let taskExpanded=false;

for (const [id, values] of [['answer-mode', ['auto','local','cloud']], ['web-mode', ['auto','on','off']]]) {
  try { const value = localStorage.getItem(`fabric-${id}`); if (values.includes(value)) $(id).value = value; } catch {}
  $(id).addEventListener('change', () => {
    try { localStorage.setItem(`fabric-${id}`, $(id).value); } catch {}
    updateRoutingHint();
  });
}
function updateRoutingHint() {
  const mode = $('answer-mode').value;
  const description = mode === 'local' ? 'Answers and tools run on this Mac.' :
    !cloudAvailable ? 'Set up a provider in Cloud to enable cloud answers.' :
    mode === 'cloud' ? 'Cloud answers eligible requests. Private context and tools stay on this Mac.' :
    'Auto uses cloud for eligible reasoning. Private context and tools stay on this Mac.';
  renderContext();
  $('routing-hint').textContent = description;
}

function renderConversation(turns) {
  const transcript = $('conversation');
  if (!turns.length) {transcript.replaceChildren();return;}
  const openActivity = new Set(
    [...transcript.querySelectorAll('details[data-turn-index][open]')].map(item => item.dataset.turnIndex)
  );
  transcript.replaceChildren();
  for (const [index, turn] of turns.entries()) {
    const row = document.createElement('article'); row.className = 'chat-turn'; row.dataset.role = turn.role;row.dataset.messageText=turn.role==='user'||turn.failure?turn.content:window.FabricChat.split(turn).answer;
    const role = document.createElement('strong'); role.textContent = turn.role === 'user' ? 'You' : turn.provider === 'cloud' ? 'Cloud' : turn.provider === 'local' ? 'This Mac' : 'Assistant';
    row.append(role);
    if (turn.role === 'user') {
      row.append(window.FabricChat.markdown(String(turn.content ?? '')));
      if (turn.attachments?.length) {
        const files = document.createElement('p');
        files.className = 'turn-meta';
        files.textContent = 'Attachments: '+turn.attachments.join(', ');
        row.append(files);
      }
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
    const steps = visibleTaskSteps(turn.steps);
    if (steps.length) {
      const details = document.createElement('details');
      details.className = 'command-receipt';
      details.dataset.turnIndex = String(index);
      details.open = openActivity.has(String(index)) || (taskExpanded && chatBusy && index === turns.length - 1);
      const title = document.createElement('summary');
      const elapsed = turn.steps?.findLast(step=>Number.isFinite(step.elapsed_seconds))?.elapsed_seconds;
      title.textContent = Number.isFinite(elapsed) ? `Task activity · ${formatTaskTime(elapsed)}` : 'Task activity';
      const list = document.createElement('div');
      appendTaskSteps(list, steps, true);
      details.append(title, list);
      row.append(details);
    }
    if(turn.role==='assistant' && (turn.model || turn.steps?.some(step=>step.agent))){const team=document.createElement('button');team.className='text-button';team.textContent='Agent structure';team.addEventListener('click',()=>{selectedAgentTurn=index;showContextTab('agent-structure');});row.append(team);}
    const copyBar = document.createElement('div');
    copyBar.className = 'message-output-actions';
    copyBar.append(window.FabricChat.copyButton(row.dataset.messageText, turn.role === 'user' ? 'Copy message' : 'Copy reply'));
    let original = turn.role === 'user' ? turn : null;
    if (turn.failure) {
      const previous = turns.slice(0, index).filter(item => item.role === 'user');
      original = previous.findLast(item => item.content === turn.original_prompt) ?? previous.at(-1);
    }
    if (original?.content?.trim()) {
      const retry = document.createElement('button');
      retry.type = 'button';
      retry.className = 'text-button';
      retry.textContent = 'Retry';
      retry.addEventListener('click', () => retryPrompt(turn.original_prompt || original.content, original.attachments));
      copyBar.append(retry);
    }
    row.append(copyBar);
    transcript.append(row);
  }
  requestAnimationFrame(()=>{transcript.scrollTop = transcript.scrollHeight;});
}

async function loadConversation() {
  const version = ++conversationLoad;
  const state = await invoke('conversation');
  if (version !== conversationLoad) return;
  if(conversationState.id!==state.id) {
    selectedAgentTurn=null;
    taskExpanded=false;
    taskState={running:false};
  }
  conversationState = state;
  chatBusy = activeRuns.has(state.id);
  if(!document.querySelector('[data-panel=chat]').hidden)$('active-view-title').textContent=state.title;
  renderConversationList(state.conversations||[]);
  $('deleted-conversations').hidden=!state.deleted?.length;
  renderDeletedConversations();
  $('prompt').value=conversationDrafts.get(state.id)||'';
  renderAttachmentDraft();
  renderPromptQueue();
  resizePrompt();
  cloudAvailable = state.cloud_available;
  $('cloud-connection-state').textContent=cloudAvailable?'Available':'Not configured';
  $('local-model-name').textContent=state.model+' · This Mac';
  $('cloud-model-name').textContent=cloudAvailable?state.cloud_model:'Optional cloud answers';

  updateRoutingHint();
  const run = activeRuns.get(state.id);
  renderConversation(run ? [...state.turns, {role:'user',content:run.text,attachments:run.attachments}, ...run.updates] : state.turns);
  pollTask().catch(() => {});
  renderActivity(state.turns);
  renderAgentTree();
  renderContext();
}

function invoke(command, args = {}) {
  if(!window.__TAURI__?.core?.invoke)return Promise.reject(new Error('Open fabric-memory with fabric-memory run.'));
  return window.__TAURI__.core.invoke('app_command',{command,args}).catch(error => { throw new Error(String(error?.message ?? error)); });
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
      searchActive ? 'Save a note or import a memory export.' : 'Add a note above. You can find it later using your own words.'));
    return;
  }
  const details = new Map(allMemories.map(memory => [memory.memory, memory]));
  for (const memory of memories) {
    const info = details.get(memory.memory) ?? memory;
    const card = document.createElement('article'); card.className = 'memory-card';card.dataset.memoryId=memory.memory;
    const top = document.createElement('div'); top.className = 'memory-top';
    const badge = document.createElement('span'); badge.className = info.translated ? 'badge translated' : 'badge';
    badge.textContent = info.translated ? 'Translated embedding' : 'MPNet · native';
    top.append(badge);
    const recordedAt = info.recorded_at ?? memory.recorded_at;
    const date = new Date(Number(recordedAt) * 1000);
    if (recordedAt && Number.isFinite(date.getTime())) {
      const recorded = document.createElement('time');
      recorded.className = 'badge';
      recorded.dateTime = date.toISOString();
      recorded.textContent = date.toLocaleDateString();
      recorded.title = 'Date recorded on this Mac';
      top.append(recorded);
    }
    const access=document.createElement('span');access.className='access-badge';const scope=(info.access?.scope||'public')==='public'?'public':'private';access.textContent=scope[0].toUpperCase()+scope.slice(1);access.title=info.access?.reason||'Previously shared memory';top.append(access);
    if (typeof memory.score === 'number') {
      const score = document.createElement('span'); score.className = 'score';
      score.textContent = `Similarity ${memory.score.toFixed(3)}`;
      top.append(score);
    }
    const remove = document.createElement('button'); remove.className = 'delete'; remove.textContent = 'Forget';
    remove.setAttribute('aria-label', `Forget memory: ${memory.text.slice(0, 60)}`);
    remove.type = 'button';
    remove.addEventListener('click', () => action(remove, () => forgetMemory(memory.memory)));
    top.append(remove);
    const text = document.createElement('p'); text.className = 'memory-text'; text.textContent = memory.text;
    card.append(top, text); list.append(card);
  }
}

async function refresh() {
  const version = ++refreshVersion;
  const [status, view] = await Promise.all([invoke('status'), invoke('memory_view')]);
  if (version !== refreshVersion) return;
  const {memories, live_memories:count} = view;
  const snapshot = JSON.stringify([count, memories]);
  const changed = snapshot !== memorySnapshot;
  memorySnapshot = snapshot;
  $('auto-sync-status').textContent = status.automatic_sync?.startsWith('Sync waiting:')?'Waiting for your Memory beacon.':status.automatic_sync || 'Automatic memory sync enabled.';$('auto-sync-status').title=status.automatic_sync||'';
  beaconOnline=status.beacon_connected===true;
  $('beacon-warning').hidden=beaconOnline;
  $('beacon-connection-badge').textContent=beaconOnline?'Connected':'Not connected';
  allMemories = memories;
  totalMemories = count;
  $('nav-count').textContent = count;
  $('connection').textContent = 'Local engine ready';
  $('connection-dot').classList.add('ready');
  const usage=status.usage;if(usage){$('usage-status').textContent=`Local: ${usage.local_calls||0} · Cloud: ${usage.cloud_calls} · Est. saved: ${usage.priced?'$'+usage.estimated_saved_usd.toFixed(4):'—'}`;$('usage-status').title=usage.note;$('usage-details').textContent=`${usage.local_calls||0} local calls · ${usage.cloud_calls} cloud calls · ${usage.local_answers} local answers since tracking began · ${usage.historical_cloud_replies||0} earlier cloud replies included · Estimated priced cloud usage $${usage.estimated_cloud_usd.toFixed(4)}${usage.unpriced_answers?` · ${usage.unpriced_answers} requests without known pricing`:""}. ${usage.note} Baseline: ${usage.baseline_model}.`;}
  $('ledger-status').textContent = `${count} ${count === 1 ? 'memory' : 'memories'} · ${status.events} durable ${status.events === 1 ? 'event' : 'events'}`;
  if (!searchActive && changed) render(allMemories);
  renderContext();
}

async function performSearch() {
  const text = $('query').value.trim();
  const version = ++searchVersion;
  if (!text) { searchActive = false; render(allMemories); return; }
  const hits = await invoke('search', { text });
  if(version !== searchVersion) return;
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
$('clear-search').addEventListener('click', () => { ++searchVersion; searchActive = false; $('query').value = ''; render(allMemories); });
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
const ATTACHMENT_LIMIT = 6 * 1024 * 1024;
function renderAttachmentDraft() {
  const files = attachmentDrafts.get(conversationState.id) || [];
  const list = $('prompt-attachments');
  list.replaceChildren();
  list.hidden = !files.length;
  for (const [index,file] of files.entries()) {
    const chip=document.createElement('button');
    chip.type='button';chip.className='secondary';
    chip.textContent=file.name+' ×';
    chip.setAttribute('aria-label','Remove '+file.name);
    if (imageExtension(file)) {
      chip.addEventListener('contextmenu', event => {
        event.stopPropagation();
        contextMenu(event, [{label:'Copy image',run:()=>copyImage(file)}, {label:'Remove image',run:()=>chip.click()}]);
      });
    }
    chip.addEventListener('click',()=>{attachmentDrafts.set(conversationState.id,files.filter((_,i)=>i!==index));renderAttachmentDraft();});
    list.append(chip);
  }
  if(files.length){const note=document.createElement('small');note.textContent='Attachments are read on this Mac.';list.append(note);}
}
function imageExtension(file) {
  return ({'image/png':'png','image/jpeg':'jpg','image/webp':'webp'})[file.type]
    || file.name?.match(/\.(png|jpe?g|webp)$/i)?.[1];
}
function addPromptFiles(incoming) {
  if(!conversationState.id || switchingConversation)return;
  const files=[...(attachmentDrafts.get(conversationState.id)||[])];
  for(const item of incoming) {
    const ext = imageExtension(item);
    const file = ext && !/\.(png|jpe?g|webp)$/i.test(item.name)
      ? new File([item], `Pasted image.${ext}`, {type:item.type})
      : item;
    if (!/\.(png|jpe?g|webp|pdf|docx|txt|md|csv|json|log|tsv|xml|ya?ml)$/i.test(file.name)) {
      message('Unsupported attachment: '+file.name,true);continue;
    }
    if (!file.size || files.length>=3 || files.reduce((n,f)=>n+f.size,0)+file.size>ATTACHMENT_LIMIT) {
      message('Attach up to three nonempty files, 6 MB total.',true);continue;
    }
    files.push(file);
  }
  attachmentDrafts.set(conversationState.id,files);
  renderAttachmentDraft();
}
function readPromptFile(file) {
  return new Promise((resolve,reject)=>{
    const reader=new FileReader();
    reader.onload=()=>resolve({name:file.name,data:String(reader.result).split(',',2)[1]});
    reader.onerror=()=>reject(Error('Could not read '+file.name));
    reader.readAsDataURL(file);
  });
}
$('attach-files').addEventListener('click',()=>$('prompt-files').click());
$('prompt-files').addEventListener('change',()=>{addPromptFiles($('prompt-files').files);$('prompt-files').value='';});
$('prompt-form').addEventListener('dragover',event=>{if(Array.from(event.dataTransfer?.types || []).includes('Files'))event.preventDefault();});
$('prompt-form').addEventListener('drop',event=>{if(event.dataTransfer?.files.length){event.preventDefault();addPromptFiles(event.dataTransfer.files);}});
async function pasteNativeImage() {
  const chat = conversationState.id;
  const image = await invoke('clipboard_image_read');
  if (!image?.data) return false;
  if (chat === conversationState.id && !switchingConversation) {
    const bytes = Uint8Array.from(atob(image.data), char=>char.charCodeAt(0));
    addPromptFiles([new File([bytes], image.name)]);
  }
  return true;
}
async function copyImage(file) {
  await invoke('clipboard_image_write', {data:(await readPromptFile(file)).data});
  message('Image copied.');
}
$('prompt').addEventListener('paste',event=>{
  const clipboard=event.clipboardData;
  const files=[...(clipboard?.files || [])];
  if (!files.length) {
    for (const item of clipboard?.items || []) {
      if (item.kind === 'file' && item.type.startsWith('image/')) {
        const file=item.getAsFile();
        if (file) files.push(file);
      }
    }
  }
  if (files.some(imageExtension) || files.some(file=>!file.type.startsWith('image/'))) {
    event.preventDefault();
    addPromptFiles(files);
  } else if (!clipboard?.getData('text/plain')) {
    event.preventDefault();
    pasteNativeImage().catch(error=>message(error.message,true));
  }
});
$('prompt-form').addEventListener('submit', async event => {
  event.preventDefault();
  const draft = $('prompt').value.trim();
  const chatId = conversationState.id;
  const files = attachmentDrafts.get(chatId) || [];
  if ((!draft && !files.length) || !chatId || switchingConversation) return;
  const answeringQuestion = taskState.pending?.kind === 'question';
  if (answeringQuestion && files.length) {
    message('Reply to the agent’s question with text. Remove the attachments first.', true);
    return;
  }
  const text = draft || 'Describe the attached files.';

  const submission = {text, mode:$('answer-mode').value, web:$('web-mode').value, files};
  const run = activeRuns.get(chatId);
  conversationDrafts.delete(chatId);
  $('prompt').value = '';
  resizePrompt();
  attachmentDrafts.delete(chatId);
  renderAttachmentDraft();

  if ((!answeringQuestion && queuedPrompts.has(chatId)) || steeringChats.has(chatId) || (run && files.length)) {
    queuePrompt(chatId, submission);
    return;
  }
  if (!run) {
    void runPrompt(text, submission.mode, submission.web, files, chatId);
    return;
  }

  steeringChats.add(chatId);
  try {
    await invoke('agent_steer', {text,conversation_id:chatId});
    if (activeRuns.get(chatId) === run) {
      run.updates.push({role:'user',content:text});
      if (conversationState.id === chatId) renderConversation([...conversationState.turns, {role:'user',content:run.text,attachments:run.attachments}, ...run.updates]);
    }
  } catch {
    // This was sent before any submissions queued while steering was pending.
    queuePrompt(chatId, submission, true);
  } finally {
    steeringChats.delete(chatId);
    startQueuedPrompt(chatId);
  }
});

function retryPrompt(text, attachments) {
  const chatId = conversationState.id;
  if (!chatId || switchingConversation) return;
  if (attachments?.length) {
    if (!$('prompt').value.trim() && !attachmentDrafts.get(chatId)?.length) {
      $('prompt').value = text;
      resizePrompt();
      $('prompt').focus();
      message('Reattach the original files, then send the prompt.');
    } else {
      message('Finish your current draft before retrying a prompt with attachments.');
    }
    return;
  }
  queuePrompt(chatId, {text, mode: $('answer-mode').value, web: $('web-mode').value, files: []});
}

function renderPromptQueue() {
  const count = queuedPrompts.get(conversationState.id)?.length || 0;
  const status = $('prompt-queue-status');
  status.hidden = count === 0;
  status.textContent = count ? `${count} queued` : '';
}

function queuePrompt(chatId, submission, first = false) {
  const queue = queuedPrompts.get(chatId) || [];
  if (first) queue.unshift(submission);
  else queue.push(submission);
  queuedPrompts.set(chatId, queue);
  renderPromptQueue();
  startQueuedPrompt(chatId);
}

function startQueuedPrompt(chatId) {
  if (activeRuns.has(chatId) || steeringChats.has(chatId)) return;
  const queue = queuedPrompts.get(chatId);
  if (!queue?.length) return;
  const next = queue.shift();
  if (!queue.length) queuedPrompts.delete(chatId);
  renderPromptQueue();
  void runPrompt(next.text, next.mode, next.web, next.files, chatId);
}

async function runPrompt(text, mode, web, files=[], chatId=conversationState.id) {
  const run = {text, attachments:files.map(file=>file.name), updates:[]};
  activeRuns.set(chatId, run);
  const title = (conversationState.conversations || []).find(chat=>chat.id===chatId)?.title
    || (conversationState.id === chatId ? conversationState.title : 'Conversation');
  if (conversationState.id === chatId) {
    chatBusy = true;
    selectedAgentTurn=null;
    taskState={running:true,steps:[]};
    renderAgentTree();
    renderConversation([...conversationState.turns, {role:'user',content:text,attachments:run.attachments}]);
    $('generation-status').hidden = true;
    $('task-panel').hidden = false;
    $('working-indicator').hidden = false;
    $('task-summary').textContent = 'Getting started…';
    $('task-review').hidden = true;
    renderTaskActivity({running:true,steps:[]});
  }
  let completion;
  try {
    const attachments = await Promise.all(files.map(readPromptFile));
    const result = await invoke('send_prompt', {text, mode, web, attachments, conversation_id:chatId});
    if (result.failure && files.length && !attachmentDrafts.has(chatId)) attachmentDrafts.set(chatId, files);
    completion = result.failure ? 'Task ended. See Activity for details.' : 'Task complete.';
    if (conversationState.id === chatId) {
      $('generation-status').hidden = !result.incomplete;
      $('generation-status').textContent = result.incomplete ? 'Reply reached its output limit. Continue to finish.' : '';
    }
  } catch (error) {
    completion = String(error?.message ?? error);
    if (files.length && !attachmentDrafts.has(chatId)) attachmentDrafts.set(chatId, files);
    if (files.length && !conversationDrafts.get(chatId)) conversationDrafts.set(chatId, text);
  } finally {
    if (conversationState.id) conversationDrafts.set(conversationState.id, $('prompt').value);
    activeRuns.delete(chatId);
    chatBusy = activeRuns.has(conversationState.id);
    await loadConversation().catch(error=>message(error.message,true));
    await updateTaskTimes().catch(()=>{});
    message(`${title} · ${completion}`);
    startQueuedPrompt(chatId);
    refresh().catch(()=>{});
  }
}
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
function selectView(view){if(view!=='setup'&&document.querySelector('.app').classList.contains('onboarding'))return;closePaperMenu();document.querySelector('.app').classList.toggle('settings-view',view!=='chat');$('active-view-title').textContent=({chat:conversationState.title||'Your conversation',memory:'Shared memory',connections:'Connections',devices:'Devices & sync',activity:'Activity',settings:'Settings',setup:'Access & local models'})[view]||view;document.querySelectorAll('[data-panel]').forEach(p=>p.hidden=p.dataset.panel!==view);document.querySelectorAll('[data-view]').forEach(b=>{if(b.dataset.view===view)b.setAttribute('aria-current','page');else b.removeAttribute('aria-current');});if(view==='activity')loadActivity().catch(e=>message(e.message,true));if(view==='connections'){loadConnections().catch(e=>message(e.message,true));loadComputerAccess().catch(e=>message(e.message,true));}if(view==='settings'){loadAgentSettings().catch(e=>message(e.message,true));loadModelStatus().catch(e=>message(e.message,true));loadCloudSettings().catch(e=>message(e.message,true));loadWorkspace().catch(e=>message(e.message,true));}if(view==='devices'){loadBeacon().catch(e=>message(e.message,true));loadDeviceUsage().catch(e=>message(e.message,true));}}
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
let loadingDeviceUsage = false;
async function loadDeviceUsage() {
  if (loadingDeviceUsage) return;
  loadingDeviceUsage = true;
  try {
    const state = await invoke('device_usage');
    const list = $('device-usage-list');
    const context = $('context-device-usage');
    list.replaceChildren();
    context.replaceChildren();
    const gib = value => (value / 1073741824).toFixed(1) + ' GiB';
    const capacity = (sample, key, label) => {
      const used = sample[key + '_used'], total = sample[key + '_total'];
      return Number.isFinite(used) && total > 0 ? label + ' ' + gib(used) + ' / ' + gib(total) : label + ' unavailable';
    };
    const devices = [{name: 'This Mac', usage:state.local, local:true},
      ...(Array.isArray(state.devices) ? state.devices.filter(device => device.id !== state.local_id) : [])];
    for (const device of devices) {
      const row = document.createElement('article');
      row.className = 'activity-item';
      const title = document.createElement('strong');
      title.textContent = device.name;
      const detail = document.createElement('p');
      const sample = device.usage;
      if (sample && Object.keys(sample).length) {
        const age = sample.sampled_at ? Math.max(0, Math.floor(Date.now() / 1000 - sample.sampled_at)) : null;
        const time = device.local ? 'Live' : age === null ? '' : age < 60 ? age + 's ago' : new Date(sample.sampled_at * 1000).toLocaleString();
        detail.textContent = [Number.isFinite(sample.cpu_percent) ? 'CPU ' + Math.round(sample.cpu_percent) + '%' : 'CPU unavailable',
          capacity(sample,'memory','Memory'), capacity(sample,'disk','Disk'), time].filter(Boolean).join(' · ');
      } else {
        detail.textContent = 'No readings yet.';
      }
      row.append(title,detail);
      if (device.local) $('local-device-usage').textContent = detail.textContent;
      else list.append(row);
      const compact = document.createElement('div');
      compact.className = 'device-context-reading';
      compact.append(title.cloneNode(true), detail.cloneNode(true));
      context.append(compact);
    }
    if (state.beacon_unavailable) {
      const note = document.createElement('p');
      note.className = 'chat-hint';
      note.textContent = 'The paired device list is unavailable. This Mac is shown above.';
      list.append(note);
    }
  } finally {
    loadingDeviceUsage = false;
  }
}
$('refresh-device-usage').addEventListener('click', event => action(event.currentTarget, loadDeviceUsage));
setInterval(() => {
  if (!document.querySelector('[data-panel="devices"]').hidden ||
      !document.querySelector('[data-panel="chat"]').hidden) loadDeviceUsage().catch(() => {});
}, 15000);
loadDeviceUsage().catch(() => {});

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
  const app = document.querySelector('.app');
  const hidden = getComputedStyle(document.querySelector('.context-panel')).display !== 'none';
  app.classList.toggle('no-context', hidden);
  app.classList.toggle('context-open', !hidden);
  $('toggle-context').setAttribute('aria-pressed', String(!hidden));
});
function renderContext(){
  const last=conversationState.turns.filter(turn=>turn.role==='assistant').at(-1);
  $('context-model').textContent=last?.model||conversationState.model||'Local model';
  $('context-executor').textContent=last?.provider==='cloud'?'Cloud provider':'This Mac';
  $('context-cloud').textContent='□ Local tool execution';
  const list=$('context-memories');list.replaceChildren();
  if(!allMemories.length){const p=document.createElement('p');p.className='chat-hint';p.textContent='No saved memories yet.';list.append(p);return;}
  allMemories.slice(-3).reverse().forEach(memory=>{
    const row=document.createElement('div');row.className='context-memory';
    const plain=document.createElement('div');plain.innerHTML=DOMPurify.sanitize(marked.parse(memory.text));const text=plain.textContent.trim();
    const p=document.createElement('p');p.textContent=text.length>140?text.slice(0,137)+'…':text;
    const source=document.createElement('small');source.textContent=memory.translated?'Synced · Translated embedding':'Saved on this Mac · MPNet';
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
    (choices[index]?.disabled ? choices.find(choice=>!choice.disabled) : choices[Math.max(0,index)])?.focus({preventScroll:true});closePaperMenu=()=>close();
  }
  trigger.addEventListener('click',()=>menu.hidden?open():close());
  trigger.addEventListener('keydown',event=>{if(['ArrowDown','ArrowUp'].includes(event.key)){event.preventDefault();open();}});
  menu.addEventListener('keydown',event=>{
    const index=choices.indexOf(document.activeElement);
    if(event.key==='Escape'){event.preventDefault();close(true);}
    else if(event.key==='Tab'){close();trigger.focus();}
    else if(['ArrowDown','ArrowUp','Home','End'].includes(event.key)){
      event.preventDefault();const next=event.key==='Home'?0:event.key==='End'?choices.length-1:(index+(event.key==='ArrowDown'?1:-1)+choices.length)%choices.length;choices[next]?.focus({preventScroll:true});
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
  if(switchingConversation)return;
  switchingConversation=true;
  conversationDrafts.set(conversationState.id,$('prompt').value);
  try{await invoke('conversation_select',{id});await loadConversation();selectView('chat');}catch(e){message(e.message,true);}finally{switchingConversation=false;}
}
async function newConversation(){
  if(switchingConversation)return;
  switchingConversation=true;
  conversationDrafts.set(conversationState.id,$('prompt').value);
  try{await invoke('conversation_create');await loadConversation();selectView('chat');$('prompt').focus();}catch(e){message(e.message,true);}finally{switchingConversation=false;}
}
function renderConversationList(items){
  const list=$('conversation-list');list.replaceChildren();
  items.forEach(chat=>{
    const button=document.createElement('button');button.className='recent-conversation conversation-action';button.textContent=chat.title;button.title=chat.title;button.dataset.conversationId=chat.id;
    if(chat.active)button.setAttribute('aria-current','page');
    const subtitle=document.createElement('small');subtitle.dataset.taskTime=chat.id;subtitle.textContent=conversationTime(chat);button.append(subtitle);button.addEventListener('click',()=>switchConversation(chat.id));list.append(button);
  });
}

async function deleteConversation(id){
  if(activeRuns.has(id) || queuedPrompts.has(id) || steeringChats.has(id))return;
  try{await invoke('conversation_delete',{id});conversationDrafts.delete(id);attachmentDrafts.delete(id);await loadConversation();message('Conversation moved to Recently deleted.');}catch(e){message(e.message,true);}
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
  if(chat)return contextMenu(event,[{label:'Open conversation',run:()=>switchConversation(chat.dataset.conversationId)},{label:'Rename conversation',disabled:chatBusy,run:()=>renameConversation(chat.dataset.conversationId)},{label:'Delete conversation',danger:true,disabled:activeRuns.has(chat.dataset.conversationId),run:()=>deleteConversation(chat.dataset.conversationId)}]);
  const input=target.closest('textarea,input');
  if(input){
    if(input.disabled){event.preventDefault();hideContextMenu();return;}
    const start=input.selectionStart??0,end=input.selectionEnd??input.value.length,selection=input.value.slice(start,end);
    const write=text=>{input.focus();if(input.selectionStart!==null)input.setRangeText(text,start,end,'end');else input.value=text;input.dispatchEvent(new Event('input',{bubbles:true}));};
    const items=[{label:'Copy',disabled:!selection,run:()=>invoke('clipboard_write',{text:selection})}];
    if(!input.readOnly){items.unshift({label:'Cut',disabled:!selection,run:async()=>{await invoke('clipboard_write',{text:selection});write('');}});items.push({label:'Paste',run:async()=>{if(input.id==='prompt' && await pasteNativeImage())return;write(await invoke('clipboard_read'));}});}
    items.push({label:'Select all',run:()=>{input.focus();input.select();}});return contextMenu(event,items);
  }
  const selection=window.getSelection()?.toString()||'';
  const items=selection?[{label:'Copy selection',run:()=>invoke('clipboard_write',{text:selection})}]:[];
  const link=target.closest('a[href]');
  if(link){items.push({label:'Open link',run:()=>invoke('open_web_source',{url:link.href})},{label:'Copy link',run:()=>invoke('clipboard_write',{text:link.href})});return contextMenu(event,items);}
  const memory=target.closest('[data-memory-id]');
  if(memory){items.push({label:'Copy memory',run:()=>invoke('clipboard_write',{text:memory.querySelector('.memory-text').textContent})},{label:'Forget memory',danger:true,disabled:memoryWrite,run:()=>forgetMemory(memory.dataset.memoryId)});return contextMenu(event,items);}
  const turn=target.closest('.chat-turn');
  if(turn){items.push({label:turn.dataset.role==='user'?'Copy message':'Copy reply',run:()=>invoke('clipboard_write',{text:turn.dataset.messageText})});if(turn.dataset.role==='user')items.push({label:'Use as prompt',disabled:chatBusy,run:()=>{$('prompt').value=turn.dataset.messageText;resizePrompt();$('prompt').focus();}});return contextMenu(event,items);}
  if(!items.length && target.closest('[data-panel=memory]'))items.push({label:'Add memory',run:()=>$('memory-text').focus()},{label:'Clear memories…',danger:true,disabled:memoryWrite,run:openClearMemories});
  else if(!items.length && (target.closest('.sidebar')||target.closest('[data-panel=chat]')))items.push({label:'New conversation',disabled:chatBusy,run:newConversation});
  if(items.length)return contextMenu(event,items);
  event.preventDefault();hideContextMenu();
});
async function forgetMemory(id) {
  if (memoryWrite) throw Error('Wait for the current memory change to finish.');
  memoryWrite = true;
  try {
    const result = await invoke('forget', {memory:id});
    ++refreshVersion;
    ++searchVersion;
    memorySnapshot = null;
    allMemories = allMemories.filter(memory => memory.memory !== id);
    for (const card of $('memories').querySelectorAll('[data-memory-id]')) {
      if (card.dataset.memoryId === id) card.remove();
    }
    message(result.forgotten ? 'Memory forgotten.' : 'This memory was already gone.');
    try {
      await refresh();
      if (searchActive) await performSearch();
    } catch (error) {
      message(`The memory is gone. The list could not refresh: ${error.message}`, true);
    }
  } finally { memoryWrite = false; }
}

let clearMemoryCount = 0;
function setClearCount(count) {
  clearMemoryCount = count;
  $('clear-memory-title').textContent = `Clear ${count} saved ${count === 1 ? 'memory' : 'memories'}?`;
  $('confirm-clear-memory').disabled = !count;
}
async function openClearMemories() {
  if (memoryWrite) throw Error('Wait for the current memory change to finish.');
  if (activeRuns.size) throw Error('Finish or stop running tasks before clearing saved memories.');
  const {live_memories:count} = await invoke('memory_view');
  if (!count) return message('There are no saved memories to clear.');
  setClearCount(count);
  $('clear-memory-error').hidden = true;
  if (!$('clear-memory-dialog').open) $('clear-memory-dialog').showModal();
  $('cancel-clear-memory').focus();
}
$('clear-memories').addEventListener('click',()=>openClearMemories().catch(e=>message(e.message,true)));
$('cancel-clear-memory').addEventListener('click',()=>$('clear-memory-dialog').close());
$('clear-memory-form').addEventListener('submit', async event => {
  event.preventDefault();
  if (memoryWrite || !clearMemoryCount) return;
  const button = $('confirm-clear-memory'), error = $('clear-memory-error');
  memoryWrite = true;
  button.disabled = true;
  button.textContent = 'Clearing…';
  error.hidden = true;
  try {
    const result = await invoke('clear_memories',{confirmation:'clear-saved-memories',expected_count:clearMemoryCount});
    ++refreshVersion;
    ++searchVersion;
    memorySnapshot = null;
    searchActive = false;
    $('query').value = '';
    allMemories = [];
    totalMemories = 0;
    $('nav-count').textContent = '0';
    render(allMemories);
    renderContext();
    $('clear-memory-dialog').close();
    message(`Cleared ${result.cleared} saved ${result.cleared === 1 ? 'memory' : 'memories'}. Conversations were kept.`);
    await refresh().catch(e => message(`Memories were cleared. The list could not refresh: ${e.message}`, true));
  } catch (failure) {
    error.textContent = failure.message;
    error.hidden = false;
    const latest = await invoke('memory_view').catch(() => null);
    if (latest) setClearCount(latest.live_memories);
  } finally {
    memoryWrite = false;
    button.disabled = !clearMemoryCount;
    button.textContent = 'Clear memories';
  }
});

document.addEventListener('pointerdown',event=>{if(!$('context-menu').contains(event.target))hideContextMenu();});
window.addEventListener('resize',hideContextMenu);document.addEventListener('scroll',hideContextMenu,true);
$('context-menu').addEventListener('keydown',event=>{const items=Array.from($('context-menu').querySelectorAll('button:not(:disabled)'));const index=items.indexOf(document.activeElement);if(event.key==='Escape'){hideContextMenu();}else if(['ArrowDown','ArrowUp'].includes(event.key)){event.preventDefault();items[(index+(event.key==='ArrowDown'?1:-1)+items.length)%items.length]?.focus();}});

let modelPoll;
async function loadModelStatus(){const state=await invoke('model_status');$('configured-model').textContent=state.model;$('load-model').disabled=state.loading;$('model-request').disabled=state.loading;$('model-progress').textContent=state.loading?`${state.status} · ${Math.round(state.progress||0)}%`:state.status==='Ready'?'':state.status;$('model-download').hidden=!state.loading;$('model-download').value=state.progress||0;if(!state.loading&&modelPoll){clearInterval(modelPoll);modelPoll=null;await loadConversation();}return state;}
$('model-form').addEventListener('submit',async event=>{event.preventDefault();$('load-model').disabled=true;try{await invoke('model_load',{prompt:$('model-request').value});await loadModelStatus();if(!modelPoll)modelPoll=setInterval(()=>loadModelStatus().catch(e=>message(e.message,true)),1500);}catch(e){message(e.message,true);await loadModelStatus();}});

async function pollTask(){
  const chatId=conversationState.id;
  if (!chatId) return;
  const state=await invoke('agent_status',{conversation_id:chatId});
  if (conversationState.id!==chatId) return;
  taskState=state;renderAgentTree();
  $('task-panel').hidden=!state.running&&!state.pending&&!(taskExpanded&&state.steps?.length);$('stop-task').hidden=!state.running;$('stop-task').disabled=state.stopped;$('stop-task').textContent=state.stopped?'Stopping…':'Stop';
  $('working-indicator').hidden=!state.running||!!state.pending;const current=state.steps?.at(-1);$('task-summary').textContent=state.stopped?(state.running?'Stopping…':'Stopped'):!state.running?(state.outcome===false?'Task ended · view activity':'Task complete · view activity'):state.pending?state.pending.summary:current?.kind==='planning'?(current.summary||'Choosing next action…'):current?.kind==='done'?'Checking results…':current?.summary||'Getting started…';
  renderTaskActivity(state);

  const question = state.pending?.kind === 'question';
  $('prompt').placeholder = question ? 'Reply to the agent’s question…' : 'Ask, remember, or continue…';
  $('task-review').hidden = !state.pending || question;
  const folderAccess = state.pending?.tool === 'workspace_access';
  $('approve-action').textContent = folderAccess ? 'Allow folder for this task' : 'Allow action';
  if (state.pending && !question) {$('review-summary').textContent=state.pending.tool;$('review-arguments').textContent=JSON.stringify(state.pending.arguments,null,2);}
}
$('approve-action').addEventListener('click',()=>reviewAction(true));$('deny-action').addEventListener('click',()=>reviewAction(false));
async function reviewAction(allow){if(!taskState.pending)return;const id=taskState.pending.id;try{await invoke('agent_review',{id,allow,conversation_id:conversationState.id});await pollTask();}catch(e){message(e.message,true);}}
$('stop-task').addEventListener('click', () => {
  invoke('agent_stop',{conversation_id:conversationState.id}).then(pollTask).catch(e => message(e.message,true));
});
async function loadConnections(){
  $('mcp-status').textContent='Checking connections…';const servers=await invoke('mcp_catalog');$('mcp-status').textContent=servers.length?servers.filter(s=>s.status==='Connected').length+' connected · '+servers.length+' configured':'No servers configured';const list=$('mcp-servers');list.replaceChildren();
  for(const server of servers){const row=document.createElement('div');row.className='service-row mcp-server-row';const copy=document.createElement('div');copy.className='service-copy';const title=document.createElement('h2');title.textContent=server.name;const state=document.createElement('p');state.textContent=server.status==='Connected'?server.tools.length+' tools · '+server.resources.length+' resources · '+server.prompts.length+' prompts':server.status;copy.append(title,state);const browse=document.createElement('button');browse.className='text-button';browse.textContent='Browse';browse.disabled=server.status!=='Connected';browse.addEventListener('click',()=>browseMCP(server));const restart=document.createElement('button');restart.className='text-button';restart.textContent='Reconnect';restart.addEventListener('click',()=>action(restart,async()=>{await invoke('mcp_restart',{server:server.name});await loadConnections();}));const edit=document.createElement('button');edit.className='text-button';edit.textContent='Edit';edit.addEventListener('click',()=>openMCPConnection(server.name).catch(e=>message(e.message,true)));const remove=document.createElement('button');remove.className='text-button';remove.textContent='Remove';remove.addEventListener('click',()=>action(remove,async()=>{const config=await invoke('mcp_config');const previous=config.mcpServers[server.name];delete config.mcpServers[server.name];await invoke('mcp_save',{config});removedMCP={name:server.name,server:previous};$('undo-mcp-removal').hidden=false;await loadConnections();message('Connection removed. Undo is available.');}));const enabled=document.createElement('button');enabled.className='text-button';enabled.textContent=server.enabled?'Disable':'Enable';enabled.addEventListener('click',()=>action(enabled,async()=>{const config=await invoke('mcp_config');if(!config.mcpServers[server.name])throw Error('Connection no longer exists.');config.mcpServers[server.name].disabled=server.enabled;await invoke('mcp_save',{config});await loadConnections();}));row.append(copy,browse,edit,enabled,restart,remove);list.append(row);}
}
async function loadComputerAccess() {
  const state = await invoke('computer_status');
  const button = $('toggle-computer');
  button.dataset.enabled = String(state.enabled);
  button.textContent = state.enabled ? 'Disable' : 'Enable';
  $('computer-state').textContent = !state.enabled ? 'Off' : state.permission_error || (state.accessibility ? 'Accessibility granted' : 'Needs Accessibility permission');
}
$('toggle-computer').addEventListener('click', event => action(event.currentTarget, async () => {
  await invoke('computer_enable', {enabled: $('toggle-computer').dataset.enabled !== 'true'});
  await loadComputerAccess();
}));
$('computer-permissions').addEventListener('click', event => action(event.currentTarget, async () => {
  await openSetup();
  $('setup-control').focus();
}));
let removedMCP=null;
let editingMCP=null,mcpSnapshot={mcpServers:{}},mcpTestRevision=0;
function mcpTransport(){return document.querySelector('input[name="mcp-transport"]:checked').value;}
function updateMCPTransport(){const local=mcpTransport()==='stdio';$('mcp-local-fields').hidden=!local;$('mcp-remote-fields').hidden=local;}
async function openMCPConnection(name=null){mcpSnapshot=await invoke('mcp_config');editingMCP=name;const server=name?mcpSnapshot.mcpServers[name]:{};$('mcp-connection-form').reset();$('mcp-name').value=name||'';$('mcp-command').value=server?.command?window.FabricMCPForm.quoteWords([server.command,...server.args||[]]):'';$('mcp-url').value=server?.url||'';$('mcp-cwd').value=server?.cwd||'';document.querySelector('input[name="mcp-transport"][value="'+(server?.url?'http':'stdio')+'"]').checked=true;$('mcp-token').value='';$('mcp-environment').value='';$('mcp-connection-title').textContent=name?'Edit MCP connection':'Add MCP connection';$('mcp-connection-result').textContent='';updateMCPTransport();mcpTestRevision++;$('mcp-connection-dialog').showModal();}
function mcpFormValue(){return window.FabricMCPForm.build({name:$('mcp-name').value,transport:mcpTransport(),command:$('mcp-command').value,url:$('mcp-url').value,token:$('mcp-token').value,environment:$('mcp-environment').value,cwd:$('mcp-cwd').value},editingMCP?mcpSnapshot.mcpServers[editingMCP]:{});}
$('configure-mcp').addEventListener('click',()=>openMCPConnection().catch(e=>message(e.message,true)));
$('close-mcp-connection').addEventListener('click',()=>{mcpTestRevision++;$('mcp-connection-dialog').close();});
document.querySelectorAll('input[name="mcp-transport"]').forEach(input=>input.addEventListener('change',updateMCPTransport));
$('mcp-connection-form').addEventListener('input',()=>{mcpTestRevision++;$('mcp-connection-result').textContent='';});
$('test-mcp-connection').addEventListener('click',()=>action($('test-mcp-connection'),async()=>{const value=mcpFormValue();const revision=++mcpTestRevision;$('mcp-connection-result').textContent='Connecting…';try{const result=await invoke('mcp_test',{server:value.server});if(revision===mcpTestRevision)$('mcp-connection-result').textContent=`Connected · ${result.tools} tools · ${result.resources} resources · ${result.prompts} prompts`;}catch(e){if(revision===mcpTestRevision)$('mcp-connection-result').textContent=e.message||String(e);}}));
$('mcp-connection-form').addEventListener('submit',event=>{event.preventDefault();action(event.submitter||$('mcp-connection-form').querySelector('[type=submit]'),async()=>{const value=mcpFormValue();const latest=await invoke('mcp_config');if(editingMCP&&JSON.stringify(latest.mcpServers[editingMCP])!==JSON.stringify(mcpSnapshot.mcpServers[editingMCP]))throw Error('This connection changed. Reopen it before saving.');if(value.name!==editingMCP&&Object.hasOwn(latest.mcpServers,value.name))throw Error('A connection with that name already exists.');if(editingMCP)delete latest.mcpServers[editingMCP];Object.defineProperty(latest.mcpServers,value.name,{value:value.server,enumerable:true,configurable:true,writable:true});await invoke('mcp_save',{config:latest});mcpTestRevision++;$('mcp-connection-dialog').close();await loadConnections();message('MCP connection saved.');});});
$('advanced-mcp').addEventListener('click',()=>action($('advanced-mcp'),async()=>{const config=await invoke('mcp_config');$('mcp-config-json').value=JSON.stringify(config,null,2);$('mcp-config-dialog').showModal();}));
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
 for(const entry of history.filter(e=>!e.phase&&!['planning','plan','answering'].includes(e.kind))){const row=document.createElement('article');row.className='activity-item';const title=document.createElement('strong');title.textContent=(entry.kind==='done'?'✓ ':entry.kind==='error'?'× ':'· ')+entry.summary;const meta=document.createElement('p');meta.textContent=[new Date(entry.time*1000).toLocaleTimeString([],{hour:'2-digit',minute:'2-digit'}),entry.tool||'This Mac',entry.action].filter(Boolean).join(' · ');row.append(title,meta);if(entry.command){const command=document.createElement('pre');command.className='tool-result action-command';command.textContent=entry.command;row.append(command);}if(entry.result!==undefined){const details=document.createElement('details');const summary=document.createElement('summary');summary.textContent='Output';const pre=document.createElement('pre');pre.className='tool-result';const value=entry.result;pre.textContent=typeof value==='string'?value:(typeof value?.stdout==='string'||typeof value?.stderr==='string')?[value.stdout,value.stderr].filter(Boolean).join('\n'):JSON.stringify(value,null,2);details.append(summary,window.FabricChat.copyButton(pre.textContent,'Copy output'),pre);row.append(details);}list.append(row);}
}

let renameTarget;
function renameConversation(id){if(chatBusy)return;const chat=(conversationState.conversations||[]).find(c=>c.id===id);if(!chat)return;renameTarget=id;$('conversation-name').value=chat.title;$('rename-dialog').showModal();$('conversation-name').select();}
$('active-view-title').addEventListener('dblclick',()=>{if(!document.querySelector('[data-panel=chat]').hidden)renameConversation(conversationState.id);});
$('rename-form').addEventListener('submit',event=>{event.preventDefault();action(event.submitter,async()=>{await invoke('conversation_rename',{id:renameTarget,title:$('conversation-name').value});$('rename-dialog').close();await loadConversation();});});
$('cancel-rename').addEventListener('click',()=>$('rename-dialog').close());

let cloudCatalogRequest=0;
function fillCloudModels(models, pinned) {
  const select = $('cloud-model');
  select.replaceChildren();
  for (const id of new Set([pinned, ...models].filter(Boolean))) {
    const option = document.createElement('option');
    option.value = id;
    option.textContent = models.includes(id) ? id : id + ' · saved';
    select.append(option);
  }
  select.value = pinned || models[0] || '';
  select.dispatchEvent(new Event('picker-update'));
}
async function loadCloudCatalog(refresh=false){const request=++cloudCatalogRequest;const state=await invoke('cloud_models',{refresh});if(request!==cloudCatalogRequest||state.provider_id&&state.provider_id!==$('cloud-provider').value)return;fillCloudModels(state.models,$('cloud-model').value);$('cloud-model-pool').textContent=`${state.models.length} available models${state.stale?' · saved catalog; refresh to update':''}`;}
async function loadCloudSettings(){cloudCatalogRequest++;const state=await invoke('cloud_settings');const providers=$('cloud-provider');providers.replaceChildren();for(const p of state.providers){const option=document.createElement('option');option.value=p.id;option.textContent=p.name;providers.append(option);}providers.value=state.provider_id;$('cloud-provider-endpoint').textContent=state.base_url;fillCloudModels([],state.model);$('cloud-automatic').checked=state.automatic;$('cloud-model-pool').textContent=state.last_selected?`Last selected: ${state.last_selected}`:'Save a token to discover available models.';$('cloud-enabled').checked=state.enabled;renderCloudCredential(state);loadAgentSettings().catch(()=>{});if(state.token_configured)loadCloudCatalog().catch(e=>{$('cloud-model-pool').textContent=e.message+' · saved model retained';});return state;}
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

$('refresh-cloud-models').addEventListener('click',()=>action($('refresh-cloud-models'),()=>loadCloudCatalog(true)));
async function loadWorkspace(){const state=await invoke('workspace_settings');$('workspace-path').value=state.path;}
$('workspace-form').addEventListener('submit',event=>{event.preventDefault();action(event.submitter,async()=>{await invoke('workspace_save',{path:$('workspace-path').value.trim(),cloud_allowed:false});$('workspace-status').textContent='Workspace ready. Ask the agent to implement, run, or test your code.';});});

$('retry-cloud-keychain').addEventListener('click',()=>action($('retry-cloud-keychain'),async()=>{await invoke('cloud_keychain_retry');await loadCloudSettings();await loadConversation();}));

let cloudKeychainPending=false;
function renderCloudCredential(state){cloudKeychainPending=!!state.keychain_pending;$('retry-cloud-keychain').hidden=!state.credential_error&&!state.keychain_pending;$('retry-cloud-keychain').disabled=!!state.keychain_pending;$('cloud-token-state').textContent=state.keychain_pending?'Waiting for Keychain access. Complete the macOS prompt or enter your token again.':state.credential_error|| (state.credential_source==='keychain'?'API token saved in Keychain.':state.credential_source==='environment'?'Using your private environment token.':'Add an API token for this provider.');$('remove-cloud-token').hidden=state.credential_source!=='keychain';}
setInterval(async()=>{if(!cloudKeychainPending)return;try{const state=await invoke('cloud_settings');if(state.provider_id!==$('cloud-provider').value)return;renderCloudCredential(state);if(!state.keychain_pending&&state.token_configured){cloudAvailable=state.enabled;updateRoutingHint();await loadCloudCatalog();}}catch{}},2000);

function visibleTaskSteps(steps) {
  return (steps || []).filter(step => !step.background && step.tool !== 'memory_search'
    && ['running','done','error','denied','approved','planning','review','rejected'].includes(step.kind));
}

let previewUrl;
let previewBlob;
let previewRequest = 0;
function clearScreenPreview() {
  previewRequest++;
  if (previewUrl) URL.revokeObjectURL(previewUrl);
  previewUrl = undefined;
  previewBlob = undefined;
  $('copy-screen-preview').hidden = true;
  const image = $('screen-preview-image');
  image.hidden = true;
  image.removeAttribute('src');
  $('screen-preview-status').textContent = '';
}
function showObservation(result) {
  clearScreenPreview();
  $('observed-window').textContent = result.window_title || result.app || 'Last browser observation';
  $('observed-controls').textContent = result.ui || 'No controls were recorded.';
  $('computer-observation').showModal();
  if (result.image_path) {
    const request = previewRequest;
    $('observed-window').textContent = 'Saved screenshot';
    $('observed-controls').textContent = result.image_path;
    invoke('computer_image', {path: result.image_path}).then(bytes => {
      if (request !== previewRequest) return;
      previewBlob = new Blob([Uint8Array.from(bytes)], {type:'image/png'});
      previewUrl = URL.createObjectURL(previewBlob);
      const image = $('screen-preview-image');
      image.src = previewUrl;
      image.hidden = false;
      $('copy-screen-preview').hidden = false;
    }).catch(error => {
      if (request === previewRequest) $('screen-preview-status').textContent = error.message;
    });
  }
}
$('show-current-observation').addEventListener('click', () => {
  const step = (taskState.steps || []).findLast(entry => entry.tool === 'computer' && entry.result?.ui);
  if (step) showObservation(step.result);
});
$('close-computer-observation').addEventListener('click', () => $('computer-observation').close());
$('computer-observation').addEventListener('close', clearScreenPreview);
$('copy-screen-preview').addEventListener('click', event=>action(event.currentTarget, async()=>{
  if (!previewBlob) return;
  await copyImage(new File([previewBlob], 'Screenshot.png', {type:'image/png'}));
}));
$('capture-screen-preview').addEventListener('click', async event => {
  const button = event.currentTarget;
  clearScreenPreview();
  const request = previewRequest;
  button.disabled = true;
  try {
    const bytes = await invoke('computer_preview');
    if (request !== previewRequest) return;
    previewBlob = new Blob([Uint8Array.from(bytes)], {type:'image/png'});
    previewUrl = URL.createObjectURL(previewBlob);
    const image = $('screen-preview-image');
    image.src = previewUrl;
    image.hidden = false;
    $('copy-screen-preview').hidden = false;
    $('screen-preview-status').textContent = 'Current screen snapshot';
  } catch (error) {
    if (request === previewRequest) $('screen-preview-status').textContent = String(error?.message ?? error);
  } finally {
    button.disabled = false;
  }
});

function appendTaskSteps(list, steps, showOutputs = false) {
  const labels = {running:'Running', done:'Completed', error:'Failed', denied:'Declined',
    approved:'Approved', review:'Review requested', rejected:'Rejected', skipped:'Reused result'};
  for (const step of steps) {
    const line = document.createElement('div');
    line.className = 'step-line ' + step.kind;
    const tool = [step.tool, step.result?.executed_action || step.action].filter(Boolean).join(' · ');
    const label = step.approval === 'automatic' ? 'Auto-approved' : labels[step.kind];
    const result = step.kind === 'done' ? step.result : null;
    const outcome = typeof result?.exit_code === 'number' ? 'exit ' + result.exit_code
      : result?.window_title || result?.navigated_url || result?.opened;
    line.textContent = [tool || step.approval ? label : null, tool, step.summary, outcome, result?.timed_out && 'Timed out', result?.truncated && 'Output truncated'].filter(Boolean).join(' · ');
    if (step.command) {
      const command = document.createElement('pre');
      command.className = 'tool-result action-command';
      command.textContent = step.command;
      line.append(command);
    }
    if (showOutputs && result != null) {
      if (step.tool === 'computer' && (result.ui || result.image_path)) {
        const view = document.createElement('button');
        view.className = 'text-button';
        view.textContent = result.image_path ? 'View screenshot' : 'What Fabric saw';
        view.addEventListener('click', () => showObservation(result));
        line.append(view);
      }
      const details = document.createElement('details');
      const title = document.createElement('summary');
      title.textContent = 'Output';
      const pre = document.createElement('pre');
      pre.className = 'tool-result';
      pre.textContent = typeof result === 'string' ? result
        : typeof result.stdout === 'string' || typeof result.stderr === 'string'
          ? [result.stdout, result.stderr].filter(Boolean).join('\n') : JSON.stringify(result, null, 2);
      details.append(title, window.FabricChat.copyButton(pre.textContent, 'Copy output'), pre);
      line.append(details);
    }
    list.append(line);
  }
}

function renderTaskActivity(state) {
  const list = $('task-steps');
  $('show-current-observation').hidden = !taskExpanded || !(state.steps || []).some(entry => entry.tool === 'computer' && entry.result?.ui);
  list.hidden = !taskExpanded;
  $('task-summary').setAttribute('aria-expanded', String(taskExpanded));
  list.replaceChildren();
  if (!taskExpanded) return;
  appendTaskSteps(list, visibleTaskSteps(state.steps));
  if (!list.childNodes.length) {
    const line = document.createElement('p');
    line.className = 'chat-hint';
    line.textContent = 'Preparing the task…';
    list.append(line);
  }
}
$('task-summary').addEventListener('click',()=>{taskExpanded=!taskExpanded;renderTaskActivity(taskState);$('task-panel').hidden=!taskState.running&&!taskState.pending&&!taskExpanded;});
async function loadAgentSettings(){const state=await invoke('agent_settings');if(document.activeElement!==$('task-budget-minutes'))$('task-budget-minutes').value=state.task_budget_minutes;$('auto-approve-actions').checked=state.auto_approve;$('agent-approval-state').textContent=state.auto_approve?'All eligible actions run automatically.':'Actions ask for review.';return state;}
$('task-budget-form').addEventListener('submit',event=>{
  event.preventDefault();
  action(event.submitter,async()=>{
    const state = await invoke('agent_settings_save',{task_budget_minutes:$('task-budget-minutes').valueAsNumber});
    $('task-budget-minutes').value = state.task_budget_minutes;
    message('Task time budget saved.');
  });
});
$('auto-approve-actions').addEventListener('change',async()=>{try{await invoke('agent_settings_save',{auto_approve:$('auto-approve-actions').checked});await loadAgentSettings();}catch(e){message(e.message,true);await loadAgentSettings();}});


$('undo-mcp-removal').addEventListener('click',()=>action($('undo-mcp-removal'),async()=>{if(!removedMCP)return;const config=await invoke('mcp_config');if(Object.hasOwn(config.mcpServers,removedMCP.name))throw Error('A connection with that name already exists.');Object.defineProperty(config.mcpServers,removedMCP.name,{value:removedMCP.server,enumerable:true});await invoke('mcp_save',{config});removedMCP=null;$('undo-mcp-removal').hidden=true;await loadConnections();}));

let selectedAgentTurn=null;
function showContextTab(panel) {
  for (const id of ['context-overview', 'agent-structure']) {
    $(id + '-tab').setAttribute('aria-selected', String(id === panel));
    $(id).hidden = id !== panel;
  }
  const app = document.querySelector('.app');
  app.classList.remove('no-context');
  app.classList.add('context-open');
  $('toggle-context').setAttribute('aria-pressed', 'true');
  renderAgentTree();
}
$('context-overview-tab').addEventListener('click', () => showContextTab('context-overview'));
$('agent-structure-tab').addEventListener('click', () => {
  selectedAgentTurn = null;
  showContextTab('agent-structure');
});
function renderAgentTree(){
  const saved=selectedAgentTurn===null?conversationState.turns?.findLast(turn=>turn.role==='assistant'):conversationState.turns?.[selectedAgentTurn];
  const live=chatBusy && selectedAgentTurn===null;
  const records=live?taskState.steps:saved?.steps;
  const nodes=new Map();
  const toolUsage=new Map();
  for (const step of records || []) {
    if (step.agent?.id) nodes.set(step.agent.id,step.agent);
    if (!step.tool || step.background || step.tool==='memory_search' ||
        !['running','done','error','failed','skipped'].includes(step.kind)) continue;
    const owner=step.agent_id || 'main';
    if (!toolUsage.has(owner)) toolUsage.set(owner,new Map());
    const name=[step.tool,step.action].filter(Boolean).join(' · ');
    const tools=toolUsage.get(owner);
    const entry=tools.get(name) || {name,count:0,state:''};
    if (step.kind==='running') entry.count++;
    entry.state=({running:'Running',done:'Complete',error:'Failed',failed:'Failed',skipped:'Reused result'})[step.kind];
    tools.set(name,entry);
  }
  if (!nodes.size && (saved?.model || toolUsage.has('main'))) {
    nodes.set('main', {id:'main', model:saved?.model, provider:saved?.provider, state:live?'running':saved?.failure?'failed':'done'});
  } else if (!live && saved?.provider === 'cloud' && saved.model && nodes.has('main') &&
      ![...nodes.values()].some(node => node.provider === 'cloud' && node.role === 'Answer generation')) {
    nodes.set('cloud-answer', {id:'cloud-answer', parent:'main', role:'Answer generation',
      model:saved.model, provider:'cloud', state:saved.failure?'failed':'done'});
  }
  if(!live && nodes.has('main')) {
    const main=nodes.get('main');
    nodes.set('main',{...main,state:saved?.failure?'failed':main.state==='running'?'done':main.state});
  }
  const tree=$('agent-tree');
  if(!nodes.size){tree.replaceChildren(empty(live?'Choosing an agent…':'No agent structure recorded','Agent models and delegation will appear here.'));return;}
  const branch=(parent,depth)=>{
    const list=document.createElement('ul');
    list.className='agent-branch';
    if(depth>3)return list;
    for(const agent of nodes.values()){
      const recordedParent = nodes.has(agent.parent) && agent.parent !== agent.id ? agent.parent : null;
      if(recordedParent!==parent)continue;
      const node=document.createElement('li'), title=document.createElement('strong'), info=document.createElement('small');
      title.textContent=agent.id==='main'?'Main agent':agent.role || `Sub-agent ${agent.id.replace(/^(local|cloud)-/,'')}`;
      info.textContent=[agent.id==='main' ? agent.role : null,agent.model,agent.provider==='cloud'?'Cloud':'This Mac',({running:'Running',done:'Complete',failed:'Failed',stopped:'Stopped',superseded:'Superseded'})[agent.state]].filter(Boolean).join(' · ');
      node.append(title,info);
      if (agent.objective) {
        const objective = document.createElement('small');
        objective.textContent = agent.objective;
        node.append(objective);
      }
      const tools=toolUsage.get(agent.id);
      if (tools?.size) {
        const usage=document.createElement('small');
        usage.textContent='Tools used';
        const entries=document.createElement('ul');
        entries.className='agent-tools';
        for (const tool of tools.values()) {
          const item=document.createElement('li');
          item.textContent=[tool.name,tool.count>1?`×${tool.count}`:null,tool.state].filter(Boolean).join(' · ');
          entries.append(item);
        }
        node.append(usage,entries);
      } else if (agent.id!=='main') {
        const usage=document.createElement('small');
        usage.textContent='No direct tool calls';
        node.append(usage);
      }
      const children=branch(agent.id,depth+1);
      if(children.childNodes.length)node.append(children);
      list.append(node);
    }
    return list;
  };
  tree.replaceChildren(branch(null,0));
  if (!live && nodes.size === 1) {
    const note = document.createElement('p');
    note.className = 'chat-hint';
    note.textContent = 'No delegation recorded for this reply.';
    tree.append(note);
  }
}

function showPermissionState(computer){
  $('setup-permission-help').hidden = !(($('setup-control').checked && !computer.accessibility) ||
    ($('setup-screen').checked && !computer.screen_recording));
  $('setup-permission-status').textContent=computer.permission_error || `Accessibility: ${computer.accessibility?'granted':'not granted'} · Screen recording: ${computer.screen_recording?'granted':'not granted'}`;
}
function setupModels(state, preferred){
  const dropdown=$('setup-model');
  dropdown.replaceChildren();
  for(const name of state.models||[]) dropdown.add(new Option(name,name));
  if(!dropdown.options.length)dropdown.add(new Option('No models available',''));
  if([...dropdown.options].some(option=>option.value===preferred))dropdown.value=preferred;
  $('setup-status').textContent=state.model_error||'';
}
async function openSetup(firstOnly=false){
  const state=await invoke('setup_state');
  if(firstOnly&&state.complete)return;
  document.querySelector('.app').classList.toggle('onboarding', !state.complete);
  $('setup-title').textContent = state.complete ? 'Access & local models' : 'Set up Fabric Memory';
  $('setup-save').textContent = state.complete ? 'Save' : 'Continue';
  $('setup-close').hidden = !state.complete;
  setupModels(state,state.model);
  $('setup-control').checked=state.computer.enabled;
  $('setup-screen').checked=state.computer.screen_sharing;
  $('setup-workspace-enabled').checked=state.workspace.enabled;
  $('setup-workspace').value=state.workspace.path;
  showPermissionState(state.computer);
  selectView('setup');
  $('setup-title').focus();
}
$('open-setup').addEventListener('click',()=>action($('open-setup'),()=>openSetup()));
$('setup-close').addEventListener('click',()=>selectView('settings'));
$('setup-refresh').addEventListener('click',()=>action($('setup-refresh'),async()=>setupModels(await invoke('setup_state'),$('setup-model').value)));
async function refreshSetupPermissions() {
  const state = await invoke('computer_status');
  if (!$('local-setup').hidden) showPermissionState(state);
}
$('setup-permissions').addEventListener('click',()=>action($('setup-permissions'),async()=>{
  const control=$('setup-control').checked, screen=$('setup-screen').checked;
  if (!control && !screen) {
    $('setup-permission-status').textContent='Select Open and control apps or Capture and record the screen first.';
    return;
  }
  try {
    showPermissionState(await invoke('computer_permissions',{control,screen}));
  } catch (error) {
    $('setup-permission-status').textContent=error.message;
  }
}));
$('setup-open-privacy').addEventListener('click',()=>action($('setup-open-privacy'),async()=>{
  $('setup-permission-help').hidden=false;
  await invoke('computer_privacy_settings');
}));
$('setup-check-access').addEventListener('click',()=>action($('setup-check-access'),refreshSetupPermissions));
for (const id of ['setup-control','setup-screen']) {
  $(id).addEventListener('change',()=>refreshSetupPermissions().catch(error=>message(error.message,true)));
}
window.addEventListener('focus',()=>{
  if (!$('local-setup').hidden) refreshSetupPermissions().catch(error=>message(error.message,true));
  else if (!document.querySelector('[data-panel="connections"]').hidden) loadComputerAccess().catch(error=>message(error.message,true));
});
$('local-setup-form').addEventListener('submit',event=>{
  event.preventDefault();
  action(event.submitter,async()=>{
    await invoke('setup_save',{model:$('setup-model').value,control:$('setup-control').checked,screen:$('setup-screen').checked,workspace:$('setup-workspace-enabled').checked,path:$('setup-workspace').value.trim()});
    const app = document.querySelector('.app');
    const firstRun = app.classList.contains('onboarding');
    app.classList.remove('onboarding');
    selectView(firstRun ? 'chat' : 'settings');
    await Promise.all([loadWorkspace(),loadModelStatus(),loadConversation()]);
  });
});
openSetup(true).catch(error=>message(error.message,true));

let cleanRoot='';
async function previewCleanup(){
  const preview=await invoke('workspace_clean_preview');
  cleanRoot=preview.root;
  $('clean-root').textContent=cleanRoot;
  const list=$('clean-items');list.replaceChildren();
  $('clean-trash').disabled=true;
  for(const item of preview.items){
    const label=document.createElement('label'), checkbox=document.createElement('input'), detail=document.createElement('span');
    label.className='clean-item';checkbox.type='checkbox';checkbox.value=item.name;
    detail.textContent=`${item.name} · ${item.project?'Folder':'File'}${item.modified?` · ${new Date(item.modified*1000).toLocaleDateString()}`:''}`;
    label.append(checkbox,detail);list.append(label);
  }
  if(!preview.items.length)list.textContent='No visible files or folders to review.';
}
$('open-workspace-clean').addEventListener('click',()=>action($('open-workspace-clean'),async()=>{
  $('clean-status').textContent='';await previewCleanup();$('workspace-clean').showModal();
}));
$('clean-close').addEventListener('click',()=>$('workspace-clean').close());
$('clean-items').addEventListener('change',()=>{$('clean-trash').disabled=!$('clean-items').querySelector('input:checked');});
$('clean-trash').addEventListener('click',()=>action($('clean-trash'),async()=>{
  const items=[...$('clean-items').querySelectorAll('input:checked')].map(box=>box.value);
  const results=await invoke('workspace_clean_trash',{root:cleanRoot,items});
  const failed=results.filter(item=>!item.trashed);
  await previewCleanup();
  $('clean-status').textContent=`Moved ${results.length-failed.length} to Trash.${failed.length?' '+failed.map(item=>item.name+': '+item.error).join('; '):''}`;
}).finally(()=>{$('clean-trash').disabled=!$('clean-items').querySelector('input:checked');}));


function formatTaskTime(value) {
  const seconds = Math.floor(value);
  const minutes = Math.floor(seconds / 60);
  return minutes ? `${minutes}m ${seconds % 60}s` : `${seconds}s`;
}
function conversationTime(chat) {
  if (Object.hasOwn(runningTimes, chat.id)) return `Running · ${formatTaskTime(runningTimes[chat.id])}`;
  if (Number.isFinite(chat.elapsed_seconds)) return `Last task · ${formatTaskTime(chat.elapsed_seconds)}`;
  return chat.messages ? `${chat.messages} messages` : 'New · This Mac';
}
async function updateTaskTimes() {
  const version = ++taskTimeLoad;
  const times = await invoke('running_tasks');
  if (version !== taskTimeLoad) return;
  runningTimes = times;
  $('open-tasks').textContent = Object.keys(runningTimes).length ? `Tasks · ${Object.keys(runningTimes).length} running` : 'Tasks';
  for (const chat of conversationState.conversations || []) {
    const label = document.querySelector(`[data-task-time="${CSS.escape(chat.id)}"]`);
    if (label) label.textContent = conversationTime(chat);
  }
  if ($('tasks-dialog').open) renderTasks();
}
function renderTasks() {
  const list = $('tasks-list');
  list.replaceChildren();
  const chats = (conversationState.conversations || []).filter(chat => Object.hasOwn(runningTimes, chat.id) || Number.isFinite(chat.elapsed_seconds));
  if (!chats.length) {
    list.textContent = 'No recent tasks.';
    return;
  }
  for (const chat of chats) {
    const row = document.createElement('button');
    row.type = 'button';
    const title = document.createElement('span');
    const duration = document.createElement('span');
    title.textContent = chat.title;
    duration.textContent = conversationTime(chat);
    row.append(title,duration);
    row.addEventListener('click', async ()=>{
      $('tasks-dialog').close();
      await switchConversation(chat.id);
      if (conversationState.id === chat.id && !activeRuns.has(chat.id)) {
        const activity = $('conversation').querySelector('.chat-turn:last-child details');
        if (activity) activity.open = true;
      }
    });
    list.append(row);
  }
}
$('open-tasks').addEventListener('click',()=>{
  renderTasks();
  $('tasks-dialog').showModal();
  updateTaskTimes().catch(()=>{});
});
setInterval(()=>{
  if (activeRuns.size) pollTask().catch(()=>{});
  if (activeRuns.size || $('tasks-dialog').open) updateTaskTimes().catch(()=>{});
},1000);
