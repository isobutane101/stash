const { invoke, convertFileSrc } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

let items=[], folders=[], lists=[], todos=[];
let mode='stash', view='all', activeFolder=null, activeTag=null, activeList=null, q='', modalId=null, lightId=null, draggingId=null;

async function load(){
  try{ items = await invoke('list_items'); }catch(e){ console.error(e); items=[]; }
  try{ folders = await invoke('list_folders'); }catch(e){ console.error(e); folders=[]; }
  try{ lists = await invoke('list_todo_lists'); }catch(e){ console.error(e); lists=[]; }
  render();
}
async function refreshItems(){ items = await invoke('list_items'); draw(); }
async function refreshFolders(){ folders = await invoke('list_folders'); draw(); }
async function refreshLists(){ lists = await invoke('list_todo_lists'); draw(); }
// Re-render whichever view is active.
function draw(){ if(mode==='todo') renderTodo(); else render(); }

const isURL=t=>/^https?:\/\/\S+$/i.test(t.trim());
function host(u){try{return new URL(u).hostname.replace(/^www\./,'');}catch(e){return'';}}
function ago(ts){const s=(Date.now()-ts)/1000;
  if(s<60)return'just now'; if(s<3600)return Math.floor(s/60)+' min ago';
  if(s<86400)return Math.floor(s/3600)+' hr ago';
  const d=Math.floor(s/86400); if(d<7)return d+' d ago';
  return new Date(ts).toLocaleDateString(undefined,{month:'short',day:'numeric'});}
function fileIcon(t){t=t||'';if(t.startsWith('image'))return'▣';if(t.includes('pdf'))return'▤';
  if(t.includes('zip')||t.includes('compress'))return'❏';if(t.includes('audio'))return'♪';
  if(t.includes('video'))return'▶';if(t.includes('text')||t.includes('json'))return'¶';return'⌉';}
function humanSize(b){if(!b)return'';const u=['B','KB','MB','GB'];let i=0;while(b>=1024&&i<3){b/=1024;i++;}return b.toFixed(b<10&&i>0?1:0)+' '+u[i];}
function esc(s){return (s||'').replace(/[&<>"']/g,m=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[m]));}

// --- data mutations: each persists through a Rust command, then re-reads from SQLite ---
async function addItem(o){
  const item=Object.assign({pinned:false,tags:[],folder:activeFolder||null},o);
  try{ await invoke('add_item',{item}); await refreshItems(); toast('Saved to stash'); }
  catch(e){ console.error(e); toast('Could not save'); }
}
function addText(t){t=t.trim();if(!t)return;addItem({type:isURL(t)?'link':'text',content:t});}
function addFiles(list){[...list].forEach(f=>{const rd=new FileReader();rd.onload=e=>{const img=f.type.startsWith('image');
  addItem({type:img?'image':'file',content:e.target.result,name:f.name,mime:f.type,size:f.size});};rd.readAsDataURL(f);});}

const cap=document.getElementById('capture'), capIn=document.getElementById('capInput');
capIn.addEventListener('input',()=>{capIn.style.height='auto';capIn.style.height=capIn.scrollHeight+'px';});
capIn.addEventListener('paste',e=>{const it=[...(e.clipboardData||{}).items||[]];const img=it.find(i=>i.type.startsWith('image'));
  if(img){e.preventDefault();const f=img.getAsFile();if(f)addFiles([f]);}});
capIn.addEventListener('keydown',e=>{if((e.metaKey||e.ctrlKey)&&e.key==='Enter'){e.preventDefault();doSave();}});
document.getElementById('saveBtn').onclick=doSave;
function doSave(){addText(capIn.value);capIn.value='';capIn.style.height='auto';}
document.getElementById('fileBtn').onclick=()=>document.getElementById('fileInput').click();
document.getElementById('fileInput').onchange=e=>{addFiles(e.target.files);e.target.value='';};
['dragenter','dragover'].forEach(ev=>cap.addEventListener(ev,e=>{e.preventDefault();cap.classList.add('drag');}));
['dragleave','drop'].forEach(ev=>cap.addEventListener(ev,e=>{e.preventDefault();if(ev==='drop')addFiles(e.dataTransfer.files);cap.classList.remove('drag');}));
['dragover','drop'].forEach(ev=>window.addEventListener(ev,e=>e.preventDefault()));

async function copyItem(it){
  if(it.type==='image'||it.type==='file'){
    try{ await invoke('copy_to_clipboard',{text:it.name||''}); toast('Filename copied'); }catch(e){ toast('Copied'); }
  }else{
    try{ await invoke('copy_to_clipboard',{text:it.content}); toast('Copied to clipboard'); }catch(e){ toast('Copied'); }
  }
}
async function copyImage(it){ try{ await invoke('copy_image_to_clipboard',{id:it.id}); toast('Image copied'); }catch(e){ console.error(e); toast('Copy failed'); } }
async function exportImage(it,fmt){ try{ const ok=await invoke('export_image_as',{id:it.id,format:fmt}); if(ok)toast('Exported '+fmt.toUpperCase()); }catch(e){ console.error(e); toast('Export failed'); } }
async function togglePin(id){const i=items.find(x=>x.id===id);
  try{ await invoke('update_item',{id,pinned:!i.pinned}); await refreshItems(); }catch(e){ console.error(e); }}
async function del(id){
  try{ await invoke('delete_item',{id}); await refreshItems(); }catch(e){ console.error(e); }}
async function download(it){
  try{ const ok=await invoke('download_item',{id:it.id}); if(ok)toast('Saved file'); }catch(e){ console.error(e); toast('Download failed'); }}
async function openLink(url){ try{ await invoke('open_url',{url}); }catch(e){ console.error(e); } }

// --- link previews: fetch og:image/title/favicon once per URL (Rust), cache the promise ---
const previewCache={};
function getPreview(url){
  if(!previewCache[url]) previewCache[url]=invoke('fetch_link_preview',{url}).catch(()=>({}));
  return previewCache[url];
}
async function loadPreview(cardEl,url){
  if(navigator.onLine===false) return;          // offline → leave the plain link
  const slot=cardEl.querySelector('.lprev'); if(!slot) return;
  const p=await getPreview(url);
  if(p&&(p.image||p.title||p.icon)) applyPreview(cardEl,p);
}
function applyPreview(cardEl,p){
  const slot=cardEl.querySelector('.lprev'); if(!slot) return;
  if(p.title){const u=cardEl.querySelector('.lurl');if(u){u.textContent=p.title;}}
  if(p.image){
    slot.innerHTML='<div class="media linkmedia"><img loading="lazy" alt="" src="'+esc(p.image)+'"></div>';
    slot.querySelector('img').onerror=()=>{slot.innerHTML='';};
  }
}
// A custom favicon: a rounded monogram tile in the app's palette, picked stably from the host.
const FAV_PALETTE=['#9c3a2f','#bb5444','#b8893f','#5c5447','#3b8c7e'];
function monogram(host){
  const h=(host||'').replace(/^www\./,'');
  const letter=(h.match(/[a-z0-9]/i)||['#'])[0].toUpperCase();
  let s=0; for(const ch of (h||'?')) s=(s*31+ch.charCodeAt(0))>>>0;
  const bg=FAV_PALETTE[s%FAV_PALETTE.length];
  const svg='<svg xmlns="http://www.w3.org/2000/svg" width="32" height="32" viewBox="0 0 32 32">'
    +'<rect width="32" height="32" rx="8" fill="'+bg+'"/>'
    +'<text x="16" y="21.5" font-family="Fraunces,Georgia,serif" font-size="17" font-weight="600" fill="#f4efe6" text-anchor="middle">'+letter+'</text>'
    +'</svg>';
  return 'data:image/svg+xml;utf8,'+encodeURIComponent(svg);
}
function faviconRow(cardEl,p){
  const host=cardEl.querySelector('.host'); if(!host||host.querySelector('img.favicon')) return;
  const img=document.createElement('img');img.className='favicon';img.alt='';
  img.src=monogram(p.host||host.textContent||'');
  host.prepend(img);
}

// ---- image lightbox: click an image to enlarge, with export/download/copy/delete ----
const imgModal=document.getElementById('imgModal');
function openLight(id){
  const it=items.find(x=>x.id===id); if(!it) return; lightId=id;
  document.getElementById('lightboxImg').src=convertFileSrc(it.content);
  const cap=document.getElementById('lightcap');
  cap.textContent=(it.name||'image')+(it.size?('  ·  '+humanSize(it.size)):'');
  const bar=document.getElementById('lightbar');bar.innerHTML='';
  const b=(label,fn,cls)=>{const el=document.createElement('button');el.className='btn sm '+(cls||'');el.textContent=label;el.onclick=fn;return el;};
  bar.append(
    b('Copy',()=>copyImage(it)),
    b('Download',()=>download(it)),
    b('Export PNG',()=>exportImage(it,'png')),
    b('Export JPEG',()=>exportImage(it,'jpeg')),
    b('Delete',async()=>{await del(it.id);closeLight();},'danger')
  );
  imgModal.classList.add('show');
}
function closeLight(){ imgModal.classList.remove('show');lightId=null;document.getElementById('lightboxImg').src=''; }
imgModal.onclick=e=>{ if(e.target===imgModal||e.target.id==='lightStage') closeLight(); };

const tagModal=document.getElementById('tagModal'), tagInput=document.getElementById('tagInput');
function openTag(id){modalId=id;renderModalTags();tagModal.classList.add('show');setTimeout(()=>tagInput.focus(),50);}
document.getElementById('tagClose').onclick=()=>tagModal.classList.remove('show');
tagModal.onclick=e=>{if(e.target===tagModal)tagModal.classList.remove('show');};
tagInput.addEventListener('keydown',async e=>{
  if(e.key!=='Enter')return;
  e.preventDefault();
  const t=tagInput.value.trim().toLowerCase().replace(/^#/,'');
  const it=items.find(x=>x.id===modalId);
  tagInput.value='';
  if(!t||!it||it.tags.includes(t)){tagInput.focus();return;}
  const tags=[...it.tags,t];
  it.tags=tags;                       // optimistic local update so you can keep typing
  renderModalTags();render();
  tagInput.focus();                   // keep focus on the field for the next tag
  try{ await invoke('update_item',{id:modalId,tags}); }
  catch(err){ console.error(err); await refreshItems(); renderModalTags(); }
});
function renderModalTags(){const it=items.find(x=>x.id===modalId);if(!it)return;const box=document.getElementById('modalChips');
  box.innerHTML=it.tags.length?'':'<span class="muted" style="padding:0">no tags yet</span>';
  it.tags.forEach(t=>{const c=document.createElement('span');c.className='chip';c.innerHTML='#'+esc(t)+' <span class="x">✕</span>';
    c.onclick=async()=>{const cur=items.find(x=>x.id===modalId);if(!cur)return;
      const tags=cur.tags.filter(x=>x!==t);cur.tags=tags;renderModalTags();render();tagInput.focus();
      try{ await invoke('update_item',{id:modalId,tags}); }catch(err){ console.error(err); await refreshItems(); renderModalTags(); }};box.appendChild(c);});}

const nf=document.getElementById('newfolder');
nf.addEventListener('keydown',async e=>{if(e.key==='Enter'){const n=nf.value.trim();if(n&&!folders.includes(n)){
  await invoke('add_folder',{name:n});nf.value='';await refreshFolders();}}});
document.querySelectorAll('.nav[data-view]').forEach(n=>n.onclick=()=>openView(n.dataset.view));
document.getElementById('search').addEventListener('input',e=>{q=e.target.value.toLowerCase();mode='stash';render();});

// New to-do list (sidebar input).
const nl=document.getElementById('newlist');
nl.addEventListener('keydown',async e=>{if(e.key==='Enter'){const n=nl.value.trim();if(!n)return;nl.value='';
  try{ const id=await invoke('add_todo_list',{name:n}); lists=await invoke('list_todo_lists'); openList(id); }catch(err){ console.error(err); }}});

// Add task (to-do panel input + button).
const ti=document.getElementById('todoInput');
async function submitTodo(){const t=ti.value.trim();if(!t||!activeList)return;ti.value='';
  try{ await invoke('add_todo',{listId:activeList,text:t}); }catch(e){ console.error(e); } refreshTodo();}
ti.addEventListener('keydown',e=>{if(e.key==='Enter'){e.preventDefault();submitTodo();}});
document.getElementById('todoAddBtn').onclick=submitTodo;
document.getElementById('todoClearBtn').onclick=async()=>{if(!activeList)return;
  try{ await invoke('clear_completed',{listId:activeList}); }catch(e){ console.error(e); } refreshTodo();};

function visible(){const searching=!!q;return items.filter(it=>{
  if(searching){ // search spans the whole collection, ignoring the active view/folder/tag
    const hay=((it.content||'')+' '+(it.name||'')+' '+(it.tags||[]).join(' ')+' '+(it.folder||'')+' '+it.type).toLowerCase();
    return hay.includes(q);
  }
  if(view==='pinned'&&!it.pinned)return false;
  if(['text','link','image','file'].includes(view)&&it.type!==view)return false;
  if(activeFolder&&it.folder!==activeFolder)return false;
  if(activeTag&&!it.tags.includes(activeTag))return false;
  return true;}).sort((a,b)=>(b.pinned-a.pinned)||(b.ts-a.ts));}

// Show the stash UI (capture + grid) or the to-do UI, depending on mode.
function applyMode(){
  const todo=mode==='todo';
  document.getElementById('capture').style.display=todo?'none':'';
  document.querySelector('.searchwrap').style.display=todo?'none':'';
  document.getElementById('grid').style.display=todo?'none':'';
  document.getElementById('todoPanel').style.display=todo?'block':'none';
  if(todo) document.getElementById('empty').style.display='none';
}

function renderSidebar(){
  document.getElementById('ct-all').textContent=items.length;
  document.getElementById('ct-pin').textContent=items.filter(i=>i.pinned).length;
  ['text','link','image','file'].forEach(t=>document.getElementById('ct-'+t).textContent=items.filter(i=>i.type===t).length);
  const stash=mode==='stash';
  document.querySelectorAll('.nav[data-view]').forEach(n=>n.classList.toggle('active',stash&&!activeFolder&&!activeTag&&n.dataset.view===view));

  // Folders — each is a drop target so you can drag a card into it.
  const fc=document.getElementById('folders');fc.innerHTML='';
  if(!folders.length)fc.innerHTML='<div class="muted">none yet</div>';
  folders.forEach(f=>{const row=document.createElement('div');row.className='folder-row';
    const n=document.createElement('div');n.className='nav'+(stash&&activeFolder===f?' active':'');
    n.innerHTML='<span class="ic">▸</span><span class="nm">'+esc(f)+'</span><span class="ct">'+items.filter(i=>i.folder===f).length+'</span>';
    n.onclick=()=>openFolder(f);
    n.addEventListener('dragover',ev=>{if(draggingId){ev.preventDefault();n.classList.add('drop');}});
    n.addEventListener('dragleave',()=>n.classList.remove('drop'));
    n.addEventListener('drop',async ev=>{ev.preventDefault();n.classList.remove('drop');
      const id=ev.dataTransfer.getData('text/plain')||draggingId;if(!id)return;
      await invoke('set_item_folder',{id,folder:f});await refreshItems();toast('Moved to '+f);});
    const x=document.createElement('button');x.className='del';x.textContent='✕';x.title='delete folder';
    x.onclick=async ev=>{ev.stopPropagation();await invoke('delete_folder',{name:f});if(activeFolder===f)activeFolder=null;
      folders=await invoke('list_folders');await refreshItems();};
    row.append(n,x);fc.appendChild(row);});

  // To-do lists.
  const lc=document.getElementById('lists');lc.innerHTML='';
  if(!lists.length)lc.innerHTML='<div class="muted">none yet</div>';
  lists.forEach(l=>{const row=document.createElement('div');row.className='folder-row';
    const n=document.createElement('div');n.className='nav'+(mode==='todo'&&activeList===l.id?' active':'');
    const left=l.total-l.done;
    n.innerHTML='<span class="ic">☑</span><span class="nm">'+esc(l.name)+'</span><span class="ct">'+(left||'')+'</span>';
    n.onclick=()=>openList(l.id);
    const x=document.createElement('button');x.className='del';x.textContent='✕';x.title='delete list';
    x.onclick=async ev=>{ev.stopPropagation();
      await invoke('delete_todo_list',{id:l.id});
      if(activeList===l.id){mode='stash';activeList=null;view='all';}
      await refreshLists();};
    row.append(n,x);lc.appendChild(row);});

  // Tags.
  const allTags=[...new Set(items.flatMap(i=>i.tags||[]))].sort();
  const tw=document.getElementById('tagwrap');tw.innerHTML=allTags.length?'':'<div class="muted">none yet</div>';
  allTags.forEach(t=>{const c=document.createElement('span');c.className='tag-pill'+(mode==='stash'&&activeTag===t?' on':'');c.textContent='#'+t;
    c.onclick=()=>toggleTag(t);tw.appendChild(c);});
}

function render(){
  mode='stash';
  applyMode();renderSidebar();
  const titles={all:'All items',pinned:'Pinned',text:'Notes',link:'Links',image:'Images',file:'Files'};
  const eyebrows={all:'The collection',pinned:'Kept close',text:'Written down',link:'Saved for later',image:'Visual',file:'Attachments'};
  const list=visible();const searching=!!q;
  let t=activeFolder?activeFolder:(titles[view]||'All items');
  document.getElementById('title').innerHTML=searching
    ? 'Search <em>&ldquo;'+esc(q)+'&rdquo;</em>'
    : (activeTag?'Tagged <em>#'+esc(activeTag)+'</em>':esc(t));
  document.getElementById('eyebrow').textContent=searching?'Across everything':(activeTag?'Tag':(activeFolder?'Folder':(eyebrows[view]||'')));
  document.getElementById('count').textContent=list.length+(list.length===1?' item':' items');
  const grid=document.getElementById('grid');grid.innerHTML='';
  document.getElementById('empty').style.display=list.length?'none':'block';
  list.forEach((it)=>{grid.appendChild(card(it));});
}

// ---- navigation helpers (each clears the others so filters never conflict) ----
function clearSearch(){ q=''; const s=document.getElementById('search'); if(s) s.value=''; }
function openView(v){ mode='stash';view=v;activeFolder=null;activeTag=null;clearSearch();render(); }
function openFolder(f){ mode='stash';view='all';activeFolder=activeFolder===f?null:f;activeTag=null;clearSearch();render(); }
function toggleTag(t){ mode='stash';activeTag=activeTag===t?null:t;view='all';activeFolder=null;clearSearch();render(); }

// ---- to-do view ----
function openList(id){ mode='todo';activeList=id;activeFolder=null;activeTag=null;refreshTodo(); }
async function refreshTodo(){
  try{ lists=await invoke('list_todo_lists'); }catch(e){ console.error(e); }
  try{ todos=activeList?await invoke('list_todos',{listId:activeList}):[]; }catch(e){ console.error(e); todos=[]; }
  renderTodo();
}
function renderTodo(){
  applyMode();renderSidebar();
  const list=lists.find(l=>l.id===activeList);
  document.getElementById('eyebrow').textContent='To-do list';
  document.getElementById('title').innerHTML=list?esc(list.name):'List';
  const done=todos.filter(t=>t.done).length;
  document.getElementById('count').textContent=todos.length?(done+' / '+todos.length+' done'):'empty';
  const box=document.getElementById('todoItems');box.innerHTML='';
  if(!todos.length){box.innerHTML='<div class="todoEmpty"><div class="big">No tasks yet</div><p>Add your first task above.</p></div>';return;}
  todos.forEach(t=>box.appendChild(todoRow(t)));
}
function todoRow(t){
  const r=document.createElement('div');r.className='todo'+(t.done?' done':'');
  const chk=document.createElement('button');chk.className='check';chk.innerHTML=t.done?'✓':'';
  chk.onclick=async()=>{ try{ await invoke('set_todo_done',{id:t.id,done:!t.done}); }catch(e){ console.error(e); } refreshTodo(); };
  const txt=document.createElement('div');txt.className='ttext';txt.textContent=t.text;
  const del=document.createElement('button');del.className='ibtn danger tdel';del.textContent='✕';del.title='Delete task';
  del.onclick=async()=>{ try{ await invoke('delete_todo',{id:t.id}); }catch(e){ console.error(e); } refreshTodo(); };
  r.append(chk,txt,del);return r;
}

function card(it){
  const c=document.createElement('div');c.className='card'+(it.pinned?' pinned':'');
  c.draggable=true;                                  // drag onto a folder in the sidebar to file it
  c.addEventListener('dragstart',e=>{draggingId=it.id;e.dataTransfer.setData('text/plain',it.id);e.dataTransfer.effectAllowed='move';c.classList.add('dragging');});
  c.addEventListener('dragend',()=>{draggingId=null;c.classList.remove('dragging');});
  let body='';
  if(it.type==='image'){body='<div class="media"><span class="badge">Image</span><img src="'+convertFileSrc(it.content)+'" alt="'+esc(it.name||'')+'"></div>';}
  else if(it.type==='file'){body='<div class="cfile"><div class="fic">'+fileIcon(it.mime)+'</div><div class="fmeta"><b>'+esc(it.name||'file')+'</b><small>'+esc((it.mime||'file').split('/')[1]||'file')+' · '+humanSize(it.size)+'</small></div></div>';}
  else if(it.type==='link'){const h=host(it.content);
    body='<div class="clink"><div class="lprev"></div>'
      +'<a class="lk" href="#" data-url="'+esc(it.content)+'"><span class="arr">↗</span><span class="lurl">'+esc(it.content)+'</span></a>'
      +'<div class="host">'+(h?esc(h):'')+'</div></div>';}
  else{body='<div class="ctext note">'+esc(it.content)+'</div>';}
  const tags=it.tags.map(t=>'<span class="tg">#'+esc(t)+'</span>').join('');
  c.innerHTML=body+'<div class="cfoot"><div class="meta"><span class="when">'+ago(it.ts)+'</span>'+tags+'</div><div class="act"></div></div>';
  const act=c.querySelector('.act');
  const mk=(sym,title,fn,cls)=>{const b=document.createElement('button');b.className='ibtn '+(cls||'');b.textContent=sym;b.title=title;
    b.onclick=e=>{e.stopPropagation();fn();};return b;};
  act.appendChild(mk(it.pinned?'★':'☆',it.pinned?'Unpin':'Pin',()=>togglePin(it.id),it.pinned?'on':''));
  act.appendChild(mk('#','Tag',()=>openTag(it.id)));
  if(it.type==='image')act.appendChild(mk('⤢','View',()=>openLight(it.id)));
  if(it.type==='image'||it.type==='file')act.appendChild(mk('⤓','Download',()=>download(it)));
  act.appendChild(mk('⧉','Copy',()=>it.type==='image'?copyImage(it):copyItem(it)));
  act.appendChild(mk('✕','Delete',()=>del(it.id),'danger'));
  if(it.type==='link'){const a=c.querySelector('.lk');if(a)a.addEventListener('click',e=>{e.preventDefault();openLink(a.dataset.url);});
    faviconRow(c,{host:host(it.content)});            // always show the monogram tile
    loadPreview(c,it.content);}
  if(it.type==='image'){const m=c.querySelector('.media');if(m){m.style.cursor='zoom-in';m.addEventListener('click',()=>openLight(it.id));}}
  else{const target=c.querySelector(it.type==='file'?'.cfile':(it.type==='link'?null:'.ctext'));
    if(target){target.style.cursor='pointer';target.addEventListener('click',()=>copyItem(it));}}
  return c;
}

let tt;function toast(m){const el=document.getElementById('toast');document.getElementById('toastmsg').textContent=m;
  el.classList.add('show');clearTimeout(tt);tt=setTimeout(()=>el.classList.remove('show'),1500);}

window.addEventListener('paste',e=>{
  const a=document.activeElement;if(a===capIn||a===tagInput||a===nf||a===document.getElementById('search'))return;
  const it=[...(e.clipboardData||{}).items||[]];const img=it.find(i=>i.type.startsWith('image'));
  if(img){const f=img.getAsFile();if(f){addFiles([f]);return;}}
  const txt=(e.clipboardData||{}).getData?(e.clipboardData).getData('text'):'';if(txt)addText(txt);});
window.addEventListener('keydown',e=>{const a=document.activeElement;
  if(e.key==='/'&&a.tagName!=='INPUT'&&a.tagName!=='TEXTAREA'){e.preventDefault();document.getElementById('search').focus();}
  if(e.key==='Escape'){tagModal.classList.remove('show');closeLight();}});

// --- background clipboard auto-capture: the Rust watcher emits, we persist + re-render ---
listen('clipboard-captured', async (e)=>{
  try{
    const p=e.payload;
    await invoke('add_item',{item:{type:p.type,content:p.content,name:p.name??null,mime:p.mime??null,size:p.size??null,tags:[],pinned:false,folder:null}});
    await refreshItems();
    toast('Captured from clipboard');
  }catch(err){ console.error(err); }
});

load();
