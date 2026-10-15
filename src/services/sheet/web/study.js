import {$,element,number} from './controls.js';
import {chart} from './drawing.js';
let archive=null,baseline=null,lastPublication=null;

export function studyPlots(publication){
  lastPublication=publication;
  if(archive&&baseline!==publication.sheet.snapshot.model_digest){archive=null;for(const id of ['study-chart','study-count','study-evidence'])$(id).replaceChildren();$('study-status').textContent='Design changed. Import a study of the current model revision.';}
  if(archive)draw();
}

$('study-file').onchange=async event=>{
  const file=event.target.files[0];if(!file||!lastPublication)return;
  const status=$('study-status');status.textContent='Verifying model, build and calculation evidence…';
  try{
    if(file.size>128*1024*1024)throw new Error('Archive exceeds the 128 MiB import limit. Use a smaller study.');
    const digest=lastPublication.sheet.snapshot.model_digest;
    const response=await fetch('/api/study/import',{method:'POST',headers:{'Content-Type':'application/json'},body:await file.text()});
    const result=await response.json();if(!response.ok)throw new Error(result.message||'Study import failed');
    if(digest!==lastPublication.sheet.snapshot.model_digest)throw new Error('Design changed during import.');
    archive=result;baseline=digest;status.textContent=`${archive.candidates.length} calculated samples · ${archive.stop_reason} · not a continuous performance boundary`;
    const ids=Object.keys(archive.baseline.parameters),x=$('study-x'),y=$('study-y');x.replaceChildren();y.replaceChildren();
    for(const name of ids){for(const choice of [x,y]){const option=element('option',name);option.value=name;choice.append(option);}}
    x.value=ids.includes('mission.distance')?'mission.distance':Object.keys(archive.candidates[0]?.updates||{})[0];
    y.value=ids.includes('payload.mass')?'payload.mass':ids[0];draw();
  }catch(error){archive=null;status.textContent=error.message;$('study-chart').replaceChildren();}
};
$('study-x').onchange=draw;$('study-y').onchange=draw;
$('study-panel').ontoggle=()=>{if(archive)draw();};

function draw(){
  if(!archive)return;const x=$('study-x').value,y=$('study-y').value;
  const points=archive.candidates.flatMap(candidate=>{
    const xv=candidate.state.values[x]?.quantity,yv=candidate.state.values[y]?.quantity;
    return xv&&yv?[{x:xv.value,y:yv.value,id:candidate.id,color:({violated:'#ad3d36',unknown:'#83939a',provisional:'#976414',satisfied:'#26775c'})[candidate.feasibility],label:`${candidate.id.slice(0,10)} · ${candidate.feasibility} · ${x} ${number(xv.value)} ${xv.unit} · ${y} ${number(yv.value)} ${yv.unit}`}]:[];
  });
  const units=name=>archive.baseline.parameters[name]?.unit||'';
  chart($('study-chart'),points,{xLabel:`${x} (${units(x)})`,yLabel:`${y} (${units(y)})`,connect:false,select:id=>inspect(id,y)});
  $('study-count').textContent=`${points.length}/${archive.candidates.length} samples with current values · red violates a hard constraint; amber is provisional`;
}

function inspect(id,metric){
  const candidate=archive.candidates.find(candidate=>candidate.id===id),node=$('study-evidence');node.replaceChildren();
  node.append(element('strong',`Candidate ${id.slice(0,12)} · ${candidate.feasibility}`));
  const runId=candidate.state.values[metric]?.run_id,run=archive.evidence[runId];
  for(const [label,data] of [['Sample and provenance',{updates:candidate.updates,model_digest:candidate.state.model_digest,build:candidate.state.build,metric:candidate.state.values[metric]}],['Requirement margins',candidate.margins],['Exact producing calculation',run||{source:'canonical input',declaration:candidate.model.parameters[metric]}]]){
    const details=element('details');details.append(element('summary',label),element('pre',JSON.stringify(data,null,2)));node.append(details);
  }
  if(run){const related=element('details');related.append(element('summary','Consumed calculation chain'));const pending=[run.id],seen=new Set();while(pending.length){const id=pending.pop();if(seen.has(id))continue;seen.add(id);const record=archive.evidence[id];if(!record)continue;const entry=element('details');entry.append(element('summary',record.node+' · '+id.slice(0,12)),element('pre',JSON.stringify(record,null,2)));related.append(entry);for(const value of Object.values(record.inputs))if(value.run_id)pending.push(value.run_id);}node.append(related);}
}
