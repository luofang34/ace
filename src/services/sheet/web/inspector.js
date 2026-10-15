import {$,element,number,quantity,traceButton,badge,inputControl} from './controls.js';

export function inspector(getSheet,commit,preview,onSelect){
  let selected=null,history=[];
  const select=(id,edit=false)=>{if(selected&&id!==selected)history.push(selected);selected=id;draw(edit);};
  $('close').onclick=()=>{$('inspector').hidden=true;selected=null;highlight([]);};
  $('back').onclick=()=>{selected=history.pop()||selected;draw();};
  function draw(edit=false){
    const sheet=getSheet();if(!sheet||!selected)return;onSelect(selected);
    $('inspector').hidden=false;const node=$('inspection');node.replaceChildren();
    const colon=selected.indexOf(':'),kind=selected.slice(0,colon),id=selected.slice(colon+1),data=lookup(sheet,kind,id);
    node.append(element('div',kind.toUpperCase(),'muted'),element('h2',sheet.trace.nodes[selected]?.label||id));
    if(kind==='parameter')parameter(node,sheet,id,commit,preview,select);
    else if(kind==='run')run(node,sheet,id,select);
    else if(kind==='requirement')requirement(node,sheet,id,select);
    else if(kind==='geometry')geometry(node,sheet,id,commit,preview,select);
    else if(kind==='analysis')analysis(node,sheet,id,select);
    else if(kind==='mission')mission(node,sheet,id,select);
    else if(data){const block=element('pre',JSON.stringify(data,null,2));node.append(block);}
    else node.append(element('p','No retained evidence for this object.','muted'));
    links(node,sheet,selected,select);
    const geometries=relatedGeometry(sheet,selected);highlight(geometries);
    if(geometries.length){node.append(element('h3','Arrangement'));for(const id of geometries)node.append(traceButton(id,'geometry:'+id,select));}
    if(edit)node.querySelector('input[type=number]')?.focus();
  }
  return {select,refresh:()=>{if(selected&&!document.activeElement?.matches('#inspection input, #inspection select'))draw();},highlight:()=>{if(selected)highlight(relatedGeometry(getSheet(),selected));}};
}

function lookup(sheet,kind,id){const keys={parameter:'parameters',geometry:'geometry',analysis:'analyses',expression:'parameters',requirement:'requirements',mass:'mass_items',mission:'missions',loading:'loading_cases',clearance:'clearances',section:'sections',objective:'objectives'};if(keys[kind])return sheet.model[keys[kind]][id];if(kind==='run')return sheet.snapshot.runs.find(run=>run.id===id);if(kind==='margin')return sheet.margins[id];if(kind==='closure')return {definition:sheet.model.closures[id],result:sheet.snapshot.closures[id]};if(kind==='operations')return sheet.model.operations;return null;}
function source(node,source){if(!source)return;node.append(element('div',`${source.kind} · ${source.reference}${source.revision?' · '+source.revision:''}`,'source'));}
function details(node,title,data){const detail=element('details'),summary=element('summary',title);detail.append(summary,element('pre',JSON.stringify(data,null,2)));node.append(detail);}

function parameter(node,sheet,id,commit,preview,select){const declaration=sheet.model.parameters[id],value=sheet.snapshot.values[id];if(!declaration)return;node.append(element('div',quantity(value?.quantity,6)),badge(value?.status||'unavailable'));if(value?.message)node.append(element('p',value.message,'muted'));node.append(element('h3','Authoritative input / result'));
  node.append(inputControl(sheet,id,commit,preview,select,false));source(node,declaration.source);node.append(element('div',`Owner: ${declaration.owner} · revision ${declaration.revision}`,'muted'));if(declaration.uncertainty)details(node,'Uncertainty',declaration.uncertainty);if(declaration.lower||declaration.upper)details(node,'Input bounds',{lower:declaration.lower,upper:declaration.upper});if(declaration.expression)details(node,'Dimensional expression',declaration.expression);if(value?.run_id){node.append(element('h3','Current calculation'),traceButton('Open exact consumed inputs and solver','run:'+value.run_id,select));}
}

function run(node,sheet,id,select){const result=sheet.snapshot.runs.find(run=>run.id===id);if(!result)return;node.append(badge(result.status));if(result.plugin){source(node,{kind:result.plugin.fidelity,reference:`${result.plugin.id}@${result.plugin.version}`});node.append(element('pre',result.plugin.code_revision));}if(result.message)node.append(element('p',result.message,'FAIL'));node.append(element('h3','Exact consumed values'));const table=element('table');
  for(const [name,input] of Object.entries(result.inputs)){const row=element('tr'),nameCell=element('td'),valueCell=element('td',quantity(input.quantity,5));nameCell.append(traceButton(name,'parameter:'+name,select),element('div',`revision ${input.revision} · ${input.source.reference}`,'muted'));row.append(nameCell,valueCell);table.append(row);}node.append(table,element('h3','Produced outputs'));
  for(const [name,value] of Object.entries(result.outputs)){const row=element('div',undefined,'metric');row.append(traceButton(name,'parameter:'+name,select),element('span',quantity(value,5)));node.append(row);}details(node,'Calculation declaration',result.calculation);details(node,'Exact semantic resources',result.resources);if(result.details!==null)details(node,'Domain evidence',result.details);if(result.artifacts.length)details(node,'Solver / mesh / load-case artifacts',result.artifacts);details(node,'Content address',{run_id:result.id,model_digest:result.model_digest,iteration:result.iteration});
}

function requirement(node,sheet,id,select){const requirement=sheet.model.requirements[id],margin=sheet.margins[id];if(!requirement)return;node.append(badge(margin.status));if(margin.provisional)node.append(element('p','This margin uses approximate evidence.','muted'));node.append(element('p',`${requirement.priority.toUpperCase()} · ${requirement.metric} ${{le:'≤',ge:'≥',eq:'='}[requirement.relation]} ${quantity(requirement.target)}`));source(node,requirement.source);node.append(element('div','Actual: '+quantity(margin.actual)),element('div','Signed margin: '+quantity(margin.absolute)));if(margin.message)node.append(element('p',margin.message));node.append(element('h3','Constrained result'),traceButton(requirement.metric,'parameter:'+requirement.metric,select));if(margin.run_id)node.append(element('p'),traceButton('Inspect supporting calculation','run:'+margin.run_id,select));if(Object.keys(requirement.condition).length)details(node,'Required operating conditions',requirement.condition);}

function geometry(node,sheet,id,commit,preview,select){const object=sheet.model.geometry[id];if(!object)return;source(node,object.source);node.append(element('div',`Role: ${typeof object.role==='string'?object.role:JSON.stringify(object.role)} · revision ${object.revision}`,'muted'));const names=editableDependencies(sheet,'geometry:'+id);node.append(element('h3','Geometry parameters'));if(!names.length)node.append(element('p','This component uses explicit geometry constants. Edit its model declaration to change them.','muted'));for(const name of names)node.append(inputControl(sheet,name,commit,preview,select));details(node,'Semantic geometry definition',object);}

function analysis(node,sheet,id,select){const analysis=sheet.model.analyses[id];if(!analysis)return;node.append(element('p',analysis.plugin+' @ '+analysis.version));for(const [heading,ports] of [['Inputs',analysis.inputs],['Outputs',analysis.outputs]]){node.append(element('h3',heading));for(const [port,name] of Object.entries(ports)){const row=element('div',undefined,'metric');row.append(traceButton(port+' → '+name,'parameter:'+name,select),element('span',quantity(sheet.snapshot.values[name]?.quantity)));node.append(row);}}const run=sheet.snapshot.runs.findLast(run=>run.node==='analysis:'+id);if(run)node.append(element('h3','Execution evidence'),traceButton('Open run '+run.id.slice(0,12),'run:'+run.id,select));details(node,'Plugin and resource contract',analysis);}

function mission(node,sheet,id,select){const mission=sheet.model.missions[id];if(!mission)return;for(const stage of mission.stages){node.append(element('h3',stage.id),traceButton(stage.kind+' → '+stage.analysis,'analysis:'+stage.analysis,select));details(node,'Stage conditions and sequencing',stage);}details(node,'Initial conditions',mission.conditions);}

function links(node,sheet,id,select){const neighbors=sheet.trace.links.filter(link=>link.from===id||link.to===id);if(!neighbors.length)return;node.append(element('h3','Provenance · both directions'));const current=neighbors.filter(link=>!link.from.startsWith('run:')&&!link.to.startsWith('run:')),history=neighbors.filter(link=>link.from.startsWith('run:')||link.to.startsWith('run:'));const render=(parent,items)=>{for(const link of items){const forward=link.from===id,other=forward?link.to:link.from,label=sheet.trace.nodes[other]?.label||other;const button=traceButton(`${forward?'→':'←'} ${link.relation.replaceAll('_',' ')} · ${label}`,other,select);button.classList.add('provenance-link');parent.append(button);}};render(node,current);if(history.length){const detail=element('details');detail.append(element('summary',`Calculation evidence (${history.length} links)`));render(detail,history);node.append(detail);}}

function walk(sheet,start){const visited=new Set(),queue=[start];while(queue.length){const id=queue.pop();if(visited.has(id))continue;visited.add(id);for(const edge of sheet.trace.links){if(edge.to===id&&['input','computes','defines','resource','mass_distribution','initial_loading','loads','phase','consumes_fuel','releases','checked','condition','sectioned','convergence_metric','measured_by','evaluated_as','supports','consumed'].includes(edge.relation))queue.push(edge.from);if(edge.from===id&&['constrains','applies_to'].includes(edge.relation))queue.push(edge.to);}}return visited;}
function editableDependencies(sheet,id){return [...walk(sheet,id)].filter(key=>key.startsWith('parameter:')&&sheet.editable.includes(key.slice(10))).map(key=>key.slice(10));}
function relatedGeometry(sheet,id){if(!sheet)return [];const visited=walk(sheet,id);const parameters=new Set([...visited].filter(id=>id.startsWith('parameter:')));for(const key of Object.keys(sheet.model.geometry)){if([...walk(sheet,'geometry:'+key)].some(id=>parameters.has(id)))visited.add('geometry:'+key);}for(const edge of sheet.trace.links)if(visited.has(edge.from)&&edge.relation==='defines'&&edge.to.startsWith('geometry:'))visited.add(edge.to);if(id.startsWith('run:')){const run=sheet.snapshot.runs.find(run=>'run:'+run.id===id);for(const key of Object.keys(run?.resources.geometry||{}))visited.add('geometry:'+key);}return [...visited].filter(id=>id.startsWith('geometry:')).map(id=>id.slice(9));}
function highlight(ids){for(const node of document.querySelectorAll('[data-geometry]'))node.classList.toggle('selected',ids.includes(node.dataset.geometry));}
