const { invoke, convertFileSrc } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

let items=[], folders=[], view='all', activeFolder=null, activeTag=null, q='', modalId=null;

async function load(){
  try{ items = await invoke('list_items'); }catch(e){ console.error(e); items=[]; }
  try{ folders = await invoke('list_folders'); }catch(e){ console.error(e); folders=[]; }
  render();
}
async function refreshItems(){ items = await invoke('list_items'); render(); }
async function refreshFolders(){ folders = await invoke('list_folders'); render(); }

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
    const img=slot.querySelector('img');
    img.onerror=()=>{slot.innerHTML='';faviconRow(cardEl,p);};
  } else { faviconRow(cardEl,p); }
}
function faviconRow(cardEl,p){
  if(!p.icon) return;
  const host=cardEl.querySelector('.host'); if(!host||host.querySelector('img')) return;
  const img=document.createElement('img');img.className='favicon';img.alt='';img.loading='lazy';img.src=p.icon;
  img.onerror=()=>img.remove();
  host.prepend(img);
}

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
document.querySelectorAll('.nav[data-view]').forEach(n=>n.onclick=()=>{view=n.dataset.view;activeFolder=null;activeTag=null;render();});
document.getElementById('search').addEventListener('input',e=>{q=e.target.value.toLowerCase();render();});

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

function render(){
  document.getElementById('ct-all').textContent=items.length;
  document.getElementById('ct-pin').textContent=items.filter(i=>i.pinned).length;
  ['text','link','image','file'].forEach(t=>document.getElementById('ct-'+t).textContent=items.filter(i=>i.type===t).length);
  document.querySelectorAll('.nav[data-view]').forEach(n=>n.classList.toggle('active',!activeFolder&&n.dataset.view===view));
  const fc=document.getElementById('folders');fc.innerHTML='';
  if(!folders.length)fc.innerHTML='<div class="muted">none yet</div>';
  folders.forEach(f=>{const row=document.createElement('div');row.className='folder-row';
    const n=document.createElement('div');n.className='nav'+(activeFolder===f?' active':'');
    n.innerHTML='<span class="ic">▸</span><span class="nm">'+esc(f)+'</span><span class="ct">'+items.filter(i=>i.folder===f).length+'</span>';
    n.onclick=()=>{view='all';activeFolder=activeFolder===f?null:f;activeTag=null;render();};
    const x=document.createElement('button');x.className='del';x.textContent='✕';x.title='delete folder';
    x.onclick=async ev=>{ev.stopPropagation();await invoke('delete_folder',{name:f});if(activeFolder===f)activeFolder=null;
      folders=await invoke('list_folders');await refreshItems();};
    row.append(n,x);fc.appendChild(row);});
  const allTags=[...new Set(items.flatMap(i=>i.tags))].sort();
  const tw=document.getElementById('tagwrap');tw.innerHTML=allTags.length?'':'<div class="muted">none yet</div>';
  allTags.forEach(t=>{const c=document.createElement('span');c.className='tag-pill'+(activeTag===t?' on':'');c.textContent='#'+t;
    c.onclick=()=>{activeTag=activeTag===t?null:t;render();};tw.appendChild(c);});
  const titles={all:'All items',pinned:'Pinned',text:'Notes',link:'Links',image:'Images',file:'Files'};
  const eyebrows={all:'The collection',pinned:'Kept close',text:'Written down',link:'Saved for later',image:'Visual',file:'Attachments'};
  const list=visible();const searching=!!q;
  let t=activeFolder?activeFolder:(titles[view]||'All items');
  document.getElementById('title').innerHTML=searching
    ? 'Search <em>&ldquo;'+esc(q)+'&rdquo;</em>'
    : (activeTag?esc(t)+' <em>#'+esc(activeTag)+'</em>':esc(t));
  document.getElementById('eyebrow').textContent=searching?'Across everything':(activeFolder?'Folder':(eyebrows[view]||''));
  document.getElementById('count').textContent=list.length+(list.length===1?' item':' items');
  const grid=document.getElementById('grid');grid.innerHTML='';
  document.getElementById('empty').style.display=list.length?'none':'block';
  list.forEach((it)=>{grid.appendChild(card(it));});
}

function card(it){
  const c=document.createElement('div');c.className='card'+(it.pinned?' pinned':'');
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
  if(it.type==='image'||it.type==='file')act.appendChild(mk('⤓','Download',()=>download(it)));
  act.appendChild(mk('⧉','Copy',()=>copyItem(it)));
  act.appendChild(mk('✕','Delete',()=>del(it.id),'danger'));
  if(it.type==='link'){const a=c.querySelector('.lk');if(a)a.addEventListener('click',e=>{e.preventDefault();openLink(a.dataset.url);});
    loadPreview(c,it.content);}
  const target=c.querySelector(it.type==='image'?'.media':(it.type==='file'?'.cfile':(it.type==='link'?null:'.ctext')));
  if(target){target.style.cursor='pointer';target.addEventListener('click',()=>copyItem(it));}
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
  if(e.key==='Escape')tagModal.classList.remove('show');});

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
