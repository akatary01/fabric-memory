const {JSDOM}=require('jsdom'),fs=require('fs'),path=require('path'),assert=require('node:assert/strict');
const root=path.resolve(__dirname,'../../apps/mac/ui');
const dom=new JSDOM(fs.readFileSync(root+'/index.html','utf8'),{runScripts:'outside-only',url:'https://fabric.test',pretendToBeVisual:true});const w=dom.window;
w.HTMLDialogElement.prototype.showModal=function(){this.open=true;};w.HTMLDialogElement.prototype.close=function(){this.open=false;};
w.ResizeObserver=class {observe(){}};w.setInterval=()=>0;w.requestAnimationFrame=fn=>fn();w.HTMLElement.prototype.scrollIntoView=()=>{};
const settings={provider:'OpenAI',provider_id:'openai',providers:[{id:'openai',name:'OpenAI'}],base_url:'https://api.openai.com/v1',model:'gpt-6.1-sol',automatic:false,enabled:true,token_configured:true,credential_source:'keychain'};
let mcpConfig={mcpServers:{}};const mcpTests=[];
const agentSettings={auto_approve:false,computer_cloud:false,provider:'OpenAI'};const saves=[];const copied=[];const promptCalls=[];const steeringCalls=[];let finishPrompt;const task={running:true,pending:null,steps:[{kind:'running',summary:'Reading browser'},{kind:'done',tool:'computer',summary:'Observed browser',result:{window_title:'Search results'}},{kind:'planning',thinking:'PRIVATE_THOUGHT'},{kind:'done',background:true,summary:'PRIVATE_MEMORY_LOOKUP'}]};
w.__TAURI__={core:{invoke:async(_,{command,args})=>{if(command==='agent_steer'){steeringCalls.push(args.text);return null;}if(command==='send_prompt'){promptCalls.push(args.text);return new Promise(resolve=>{finishPrompt=resolve;});}if(command==='device_usage')return {local:{cpu_percent:33,memory_used:2147483648,memory_total:8589934592},devices:[{id:'phone',name:'Test phone',usage:{disk_used:10737418240,disk_total:68719476736,sampled_at:1}}]};if(command==='computer_status')return {enabled:false};if(command==='mcp_config')return JSON.parse(JSON.stringify(mcpConfig));if(command==='mcp_save'){mcpConfig=JSON.parse(JSON.stringify(args.config));return mcpConfig;}if(command==='mcp_test'){mcpTests.push(args.server);return {tools:3,resources:1,prompts:0};}if(command==='mcp_catalog')return Object.entries(mcpConfig.mcpServers).map(([name,s])=>({name,enabled:!s.disabled,status:'Connected',tools:[],resources:[],prompts:[]}));if(command==='clipboard_write'){copied.push(args.text);return null;}if(command==='agent_history')return [{kind:'done',summary:'Ran command',result:{stdout:'hello\n',stderr:'',exit_code:0},time:1}];if(command==='agent_settings')return agentSettings;if(command==='agent_settings_save'){saves.push(args);Object.assign(agentSettings,args);return agentSettings;}if(command==='agent_status')return task;if(command==='cloud_settings')return settings;if(command==='cloud_models')return {models:['gpt-6-luna','gpt-6.1-sol'],provider:'OpenAI'};if(command==='conversation')return {id:'test',title:'Test',turns:[],conversations:[],deleted:[]};if(command==='list_memories')return [];if(command==='memory_view')return {live_memories:0,memories:[]};if(command==='setup_state')return {complete:true};if(command==='status')return {live_memories:0,events:0};throw Error('Fixture unavailable: '+command);}}};
for(const file of ['vendor/marked.js','vendor/purify.min.js','chat-render.js','mcp-form.js','app.js'])w.eval(fs.readFileSync(root+'/'+file,'utf8'));
(async()=>{
 await w.loadCloudSettings();await w.loadCloudCatalog();
 const $=id=>w.document.getElementById(id);
 $('settings-cloud-tab').click();assert.equal($('settings-general').hidden,true);assert.equal($('settings-cloud').hidden,false);assert.equal($('settings-cloud-tab').getAttribute('aria-selected'),'true');
 assert.equal($('cloud-model').tagName,'SELECT');assert.equal($('cloud-model').options.length,2);await Promise.resolve();assert.equal($('cloud-model-options').querySelectorAll('button').length,2);assert.equal($('cloud-model-picker').textContent,'gpt-6.1-sol');$('cloud-model-picker').click();[...$('cloud-model-options').querySelectorAll('button')].find(b=>b.dataset.value==='gpt-6-luna').click();assert.equal($('cloud-model').value,'gpt-6-luna');$('cloud-model').value='gpt-6.1-sol';$('cloud-model').dispatchEvent(new w.Event('change'));assert.equal($('cloud-provider-picker').textContent,'OpenAI');assert.equal($('cloud-model').value,'gpt-6.1-sol');assert.equal($('cloud-model').disabled,false);
 settings.keychain_pending=true;await w.loadCloudSettings();assert.equal($('retry-cloud-keychain').disabled,true);assert($('cloud-token-state').textContent.includes('Waiting for Keychain'));settings.keychain_pending=false;settings.credential_error='Unlock Keychain';await w.loadCloudSettings();assert.equal($('retry-cloud-keychain').hidden,false);assert.equal($('retry-cloud-keychain').disabled,false);delete settings.credential_error;await w.loadCloudSettings();
 $('cloud-automatic').checked=true;$('cloud-automatic').dispatchEvent(new w.Event('change'));assert.equal($('cloud-model').disabled,true);
 $('add-cloud-provider').click();assert.equal($('provider-form').hidden,false);$('cancel-provider').click();assert.equal($('provider-form').hidden,true);
 w.renderConversation([{role:'assistant',content:'Detail: ```html <script>bad()</script> ```',original_prompt:'Please fix this',failure:{settings:true,local:true}}]);assert.equal(w.document.querySelectorAll('#conversation iframe').length,0);assert(w.document.querySelector('#conversation').textContent.includes('<script>'));assert.equal(w.document.querySelectorAll('#conversation button').length,5);
 [...w.document.querySelectorAll('#conversation button')].find(b=>b.textContent==='Edit request').click();assert.equal($('prompt').value,'Please fix this');
 await w.loadAgentSettings();assert.equal($('auto-approve-actions').checked,false);assert.equal($('cloud-computer'),null);assert.equal($('workspace-cloud'),null);
 $('auto-approve-actions').checked=true;$('auto-approve-actions').dispatchEvent(new w.Event('change'));await Promise.resolve();await Promise.resolve();assert.equal(saves.at(-1).auto_approve,true);assert.equal(agentSettings.computer_cloud,false);

 await w.pollTask();assert.equal($('task-steps').hidden,true);$('task-summary').click();assert.equal($('task-steps').hidden,false);assert.equal($('task-summary').getAttribute('aria-expanded'),'true');assert($('task-steps').textContent.includes('Search results'));assert(!$('task-steps').textContent.includes('PRIVATE_THOUGHT'));assert(!$('task-steps').textContent.includes('PRIVATE_MEMORY_LOOKUP'));
 task.steps.push({kind:'approved',approval:'automatic',summary:'Run tests'});await w.pollTask();assert($('task-steps').textContent.includes('Auto-approved'));assert.equal($('task-steps').hidden,false);
 task.running=false;task.outcome=true;await w.pollTask();assert.equal($('task-panel').hidden,false);assert($('task-summary').textContent.includes('complete'));$('task-summary').click();assert.equal($('task-steps').hidden,true);assert.equal($('task-panel').hidden,true);
 w.renderConversation([{role:'assistant',content:'<think>PRIVATE_PLAN</think>Visible output'}]);w.document.querySelector('#conversation .copy-output').click();await Promise.resolve();assert.equal(copied.at(-1),'Visible output');
 const code=w.FabricChat.markdown('```python\nprint("hello")\n```');w.document.body.append(code);code.querySelector('.copy-output').click();await Promise.resolve();assert(copied.at(-1).includes('print("hello")'));
 await w.loadActivity();w.document.querySelector('#activity-list .copy-output').click();await Promise.resolve();assert.equal(copied.at(-1),'hello\n');
 w.renderConversation([{role:'assistant',content:'Command complete',steps:[
   {kind:'rejected',tool:'browser_search',summary:'Unavailable tool'},
   {kind:'done',tool:'run_code',result:{exit_code:0,stdout:'last success'}}
 ]}]);
 const activity=w.document.querySelector('.command-receipt');
 assert.equal(activity.querySelector('summary').textContent,'Task activity');
 assert(activity.textContent.includes('browser_search'));
 assert(activity.textContent.includes('exit 0'));
 activity.querySelector('.copy-output').click();
 await Promise.resolve();
 assert.equal(copied.at(-1),'last success');

 await w.loadConversation();
 const submit=text=>{$('prompt').value=text;$('prompt-form').dispatchEvent(new w.Event('submit',{cancelable:true}));};
 const tick=()=>new Promise(resolve=>setImmediate(resolve));
 submit('first prompt');submit('second prompt');await tick();
 assert.deepEqual(promptCalls,['first prompt']);
 assert.deepEqual(steeringCalls,['second prompt']);
 assert(!$('prompt').readOnly);
 assert($('conversation').textContent.includes('second prompt'));
 $('prompt').value='draft typed during the first run';
 $('prompt').dispatchEvent(new w.Event('input'));
 finishPrompt({});await tick();
 assert.deepEqual(promptCalls,['first prompt']);
 assert.equal($('prompt').value,'draft typed during the first run');
 await w.loadDeviceUsage();
 assert($('local-device-usage').textContent.includes('CPU 33%'));
 assert($('context-device-usage').textContent.includes('CPU 33%'));
 assert($('context-device-usage').textContent.includes('Test phone'));
 assert($('device-usage-list').textContent.includes('Test phone'));
 assert($('device-usage-list').textContent.includes('Disk 10.0 GiB / 64.0 GiB'));
 w.renderConversation([{role:'assistant',content:'Observed the browser',steps:[
   {kind:'done',tool:'computer',action:'observe',result:{window_title:'Chrome',ui:'AXLink · Example'}}
 ]}]);
 [...$('conversation').querySelectorAll('button')].find(button=>button.textContent==='What Fabric saw').click();
 assert.equal($('observed-window').textContent,'Chrome');
 assert.equal($('observed-controls').textContent,'AXLink · Example');
 $('close-computer-observation').click();
 await w.openMCPConnection();assert.equal($('mcp-connection-dialog').open,true);assert.equal($('mcp-local-fields').hidden,false);$('mcp-name').value='Local tools';$('mcp-command').value='node "/fixture/server.js"';$('test-mcp-connection').click();await new Promise(r=>setImmediate(r));assert.equal(mcpTests.length,1);assert($('mcp-connection-result').textContent.includes('3 tools'));assert.deepEqual(mcpConfig,{mcpServers:{}});$('mcp-connection-form').dispatchEvent(new w.Event('submit',{cancelable:true}));await new Promise(r=>setImmediate(r));assert.equal(mcpConfig.mcpServers['Local tools'].command,'node');assert.equal($('mcp-connection-dialog').open,false);
 await w.openMCPConnection('Local tools');$('mcp-name').value='Renamed tools';$('mcp-connection-form').dispatchEvent(new w.Event('submit',{cancelable:true}));await new Promise(r=>setImmediate(r));assert(!mcpConfig.mcpServers['Local tools']);assert(mcpConfig.mcpServers['Renamed tools']);[...$('mcp-servers').querySelectorAll('button')].find(b=>b.textContent==='Remove').click();await new Promise(r=>setImmediate(r));assert.equal(Object.keys(mcpConfig.mcpServers).length,0);$('undo-mcp-removal').click();await new Promise(r=>setImmediate(r));assert(mcpConfig.mcpServers['Renamed tools']);await w.openMCPConnection();const remote=w.document.querySelector('input[name="mcp-transport"][value="http"]');remote.checked=true;remote.dispatchEvent(new w.Event('change'));assert.equal($('mcp-remote-fields').hidden,false);assert.equal($('mcp-local-fields').hidden,true);$('close-mcp-connection').click();
 console.log('Settings UI: tabs, model dropdown, provider setup, safe failures, auto-approval and removed cloud-control opt-ins, expandable live activity, hidden private reasoning/memory, and completion passed.');w.close();
})().catch(e=>{console.error(e);w.close();process.exitCode=1;});
