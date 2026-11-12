const {JSDOM}=require('jsdom'),fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const root=path.resolve(__dirname,'../../apps/mac/ui');
const {window:w}=new JSDOM(fs.readFileSync(root+'/index.html','utf8'),{runScripts:'outside-only',url:'https://fabric.test'});
w.HTMLDialogElement.prototype.showModal=function(){this.open=true;};
w.HTMLDialogElement.prototype.close=function(){this.open=false;};
w.ResizeObserver=class {observe(){}};
w.requestAnimationFrame=fn=>fn();w.setInterval=()=>0;
const $=id=>w.document.getElementById(id), tick=()=>new Promise(resolve=>setImmediate(resolve));
let rows=['first','second','third'].map(memory=>({memory,text:memory}));
let staleView, releaseStale, clearRejected=true;
const calls=[];
w.__TAURI__={core:{invoke:async(_,{command,args})=>{
 calls.push({command,args});
 switch(command){
  case 'setup_state':return {complete:true};
  case 'conversation':return {id:'test',title:'Test',turns:[],conversations:[],deleted:[]};
  case 'status':return {events:rows.length};
  case 'memory_view':
   if(staleView){const result=staleView;staleView=null;return new Promise(resolve=>{releaseStale=()=>resolve(result);});}
   return {live_memories:rows.length,memories:[...rows]};
  case 'forget': rows=rows.filter(row=>row.memory!==args.memory);return {forgotten:true};
  case 'clear_memories':
   if(clearRejected){clearRejected=false;rows.push({memory:'synced',text:'synced'});throw 'Memory changed while the confirmation was open.';}
   assert.equal(args.expected_count,rows.length);
   rows=[];return {cleared:2};
  default:return {};
 }
}}};
for(const file of ['vendor/marked.js','vendor/purify.min.js','chat-render.js','mcp-form.js','app.js'])w.eval(fs.readFileSync(root+'/'+file,'utf8'));
(async()=>{
 await tick();
 assert.equal($('memory-count').textContent,'3');
 const first=$('memories').querySelector('button');
 await w.refresh();
 assert.equal($('memories').querySelector('button'),first);
 staleView={live_memories:3,memories:[...rows]};
 const stale=w.refresh();
 first.click();await tick();releaseStale();await stale;
 assert.equal($('memory-count').textContent,'2');
 assert(!$('memories').querySelector('[data-memory-id=first]'));
 const second=$('memories').querySelector('[data-memory-id=second]');
 second.dispatchEvent(new w.MouseEvent('contextmenu',{bubbles:true,cancelable:true}));
 [...$('context-menu').querySelectorAll('button')].find(button=>button.textContent==='Forget memory').click();
 await tick();assert(!$('memories').querySelector('[data-memory-id=second]'));
 $('clear-memories').click();await tick();
 assert($('clear-memory-dialog').open);
 $('confirm-clear-memory').click();await tick();
 assert($('clear-memory-dialog').open);
 assert(!$('clear-memory-error').hidden);
 assert.match($('clear-memory-error').textContent,/Memory changed/);
 assert.match($('clear-memory-title').textContent,/2 saved memories/);
 $('confirm-clear-memory').click();await tick();
 assert(!$('clear-memory-dialog').open);
 assert.equal($('memory-count').textContent,'0');

 w.renderConversation([{role:'user',content:'Visit [the example](https://example.com/path)'}]);
 $('conversation').querySelector('a').click();await tick();
 assert.equal(calls.findLast(call=>call.command==='open_web_source').args.url,'https://example.com/path');
 const event=new w.Event('drop',{bubbles:true,cancelable:true});
 event.dataTransfer={files:[new w.File(['sample'],'note.txt',{type:'text/plain'}),new w.File(['image'],'mock.png',{type:'image/png'})]};
 $('prompt-form').dispatchEvent(event);
 assert.equal($('prompt-attachments').querySelectorAll('button').length,2);
 $('prompt-attachments').querySelectorAll('button')[1].click();
 const paste=new w.Event('paste',{bubbles:true,cancelable:true});
 paste.clipboardData={files:[],items:[{kind:'file',type:'image/png',getAsFile:()=>new w.File(['clipboard image'],'image',{type:'image/png'})}]};
 $('prompt').dispatchEvent(paste);
 assert(paste.defaultPrevented);
 assert.match($('prompt-attachments').textContent,/Pasted image.png/);
 const imageChip=$('prompt-attachments').querySelectorAll('button')[1];
 imageChip.dispatchEvent(new w.MouseEvent('contextmenu',{bubbles:true,cancelable:true}));
 $('context-menu').querySelector('button').click();
 for(let i=0;i<15&&!calls.some(call=>call.command==='clipboard_image_write');i++)await tick();
 const imageCopy=calls.find(call=>call.command==='clipboard_image_write');
 assert.equal(Buffer.from(imageCopy.args.data,'base64').toString(),'clipboard image');
 const textPaste=new w.Event('paste',{bubbles:true,cancelable:true});
 textPaste.clipboardData={files:[],items:[],getData:()=> 'ordinary text'};
 $('prompt').dispatchEvent(textPaste);
 assert(!textPaste.defaultPrevented);
 $('prompt-form').dispatchEvent(new w.Event('submit',{bubbles:true,cancelable:true}));
 for(let i=0;i<15&&!calls.some(call=>call.command==='send_prompt');i++)await tick();
 const send=calls.find(call=>call.command==='send_prompt');
 assert.equal(send.args.text,'Describe the attached files.');
 assert.equal(send.args.attachments[0].name,'note.txt');
 assert.equal(Buffer.from(send.args.attachments[0].data,'base64').toString(),'sample');
 assert.equal(send.args.attachments[1].name,'Pasted image.png');
 assert.equal(Buffer.from(send.args.attachments[1].data,'base64').toString(),'clipboard image');
 assert.equal(send.args.attachments.length,2);
 console.log('Memory UI: individual and context-menu Forget, stale refresh, bulk retry, links and attachment submission passed.');
 await tick();w.close();
})().catch(error=>{console.error(error);w.close();process.exitCode=1;});
