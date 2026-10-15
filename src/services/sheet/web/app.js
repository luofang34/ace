import {$,element,number,quantity,metric} from './controls.js';
import {render,configuration,focusConstraint} from './view.js';
import {inspector} from './inspector.js';

let publication=null,previewPending=null,previewActive=false,previewToken=null;
const inspection=inspector(()=>publication?.sheet,commit,preview,id=>focusConstraint(publication?.sheet,id,select));
const select=(id,edit)=>inspection.select(id,edit);

function notice(message){$('notice').hidden=!message;$('notice').textContent=message||'';}
async function request(path,body){const response=await fetch(path,{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(body)});const text=await response.text();let data={};try{data=text?JSON.parse(text):{};}catch{data={message:text};}if(!response.ok)throw new Error(data.message||`Request failed (${response.status})`);return data;}

async function commit(updates,revision){if(!publication)return;previewToken=null;previewPending=null;$('preview').hidden=true;notice(null);try{await request('/api/edit',{expected_revision:revision??publication.sheet.revision,updates});}catch(error){notice(error.message);}}

function preview(updates,revision){if(!updates){previewToken=null;previewPending=null;$('preview').hidden=true;return;}if(!publication)return;const token={};previewToken=token;previewPending={updates,revision:revision??publication.sheet.revision,token};$('preview').hidden=false;$('preview-values').replaceChildren(element('div','Evaluating fast model…','muted'));drainPreview();}
async function drainPreview(){if(previewActive||!previewPending)return;previewActive=true;const job=previewPending;previewPending=null;try{const result=await request('/api/preview',{expected_revision:job.revision,updates:job.updates});if(previewToken===job.token&&publication.sheet.revision===result.base_revision)showPreview(result.sheet,job.updates);}catch(error){if(previewToken===job.token)$('preview-values').replaceChildren(element('div',error.message,'muted'));}finally{previewActive=false;if(previewPending)drainPreview();}}
function showPreview(sheet,updates){const node=$('preview-values');node.replaceChildren();const interesting=new Set([...Object.keys(updates),'wing.aspect_ratio','weight.oew','weight.mtow','mission.block_fuel','economics.casm']);for(const name of interesting){const before=publication.sheet.snapshot.values[name]?.quantity,after=sheet.snapshot.values[name]?.quantity;if(!after)continue;const delta=before&&before.unit===after.unit?after.value-before.value:null;const row=metric(name,quantity(after,4),'parameter:'+name,select);if(delta!==null)row.append(element('small',`${delta>=0?'+':''}${number(delta,4)}`,'muted'));node.append(row);}const violations=Object.values(sheet.margins).filter(margin=>margin.absolute?.value<0);if(violations.length)node.append(element('div',violations.map(margin=>sheet.model.requirements[margin.requirement].name+': '+quantity(margin.absolute)).join(' · '),'FAIL'));}

function receive(next){publication=next;render(next,{select,commit,preview});inspection.refresh();inspection.highlight();if(next.error)notice(next.error);else if(next.activity==='ready')notice(null);if(previewToken&&next.activity==='computing'){previewToken=null;previewPending=null;$('preview').hidden=true;}}

$('parameter-search').oninput=()=>{if(publication)configuration(publication.sheet,select,commit,preview);};
$('save').onclick=async()=>{if(!publication)return;notice(null);try{await request('/api/save',{expected_revision:publication.sheet.revision});}catch(error){notice(error.message);}};
$('export').onclick=()=>{const anchor=element('a');anchor.href='/api/model';anchor.download='vehicle.json';anchor.click();};
$('print').onclick=()=>window.print();

const events=new EventSource('/api/events');events.onmessage=event=>{try{receive(JSON.parse(event.data));}catch(error){notice('Unable to read the design update: '+error.message);}};
events.onerror=()=>{$('activity').textContent='RECONNECTING';$('activity').className='badge unavailable';$('save').disabled=true;};
window.addEventListener('pagehide',()=>events.close());

$('cancel').onclick=async()=>{if(!publication?.job)return;try{await request('/api/cancel',{job_id:publication.job.id});}catch(error){notice(error.message);}};
$('run').onclick=async()=>{if(!publication)return;try{await request('/api/run',{expected_revision:publication.sheet.revision});}catch(error){notice(error.message);}};
