import {$,element,number,quantity,traceButton,metric,badge,empty,current,panel,inputControl} from './controls.js';
import {arrangement,sectionSvg,chart} from './drawing.js';
import {missionStates} from './trajectory.js';
import {studyPlots} from './study.js';

export function render(publication,{select,commit,preview}){
  const sheet=publication.sheet,model=sheet.model;$('title').textContent=model.name;$('identity').textContent=`${model.domain} · revision ${sheet.revision} · ${model.id}`;
  $('cancel').disabled=publication.job?.state!=='running';$('run').disabled=publication.activity==='computing';$('activity').textContent=publication.activity.toUpperCase();$('activity').className='badge '+publication.activity;
  $('saved').textContent=publication.saved_digest===sheet.snapshot.model_digest?'Saved':'Unsaved';$('save').disabled=publication.activity!=='ready';
  const geometry=panel(sheet,'vehicle.geometry_envelopes'),mass=panel(sheet,'vehicle.mass_properties'),mission=panel(sheet,'vehicle.mission_sequence'),operations=panel(sheet,'vehicle.trip_cost')||panel(sheet,'vehicle.operations');
  requirements(sheet,select);statements(sheet,mass,operations,select);plots(geometry,mission,select);massProperties(mass,select,preferredFocus(sheet,'loading'));missionStates(sheet,mission,select,()=>massProperties(mass,select));closures(sheet,select);
  arrangement($('arrangement'),geometry?.details,sheet,select,preview,commit);criticalSections(sheet,geometry,select,preferredFocus(sheet,'sections'));
  for(const [id,results] of [['arrangement',[geometry]],['mass',[mass]],['mission',[mission]],['sections',[geometry]],['statement',[mass,operations]],['plots',[geometry,mission]]]){const stale=results.some(result=>result&&(result.historical||['stale','blocked','unavailable','nonconverged'].includes(result.status)));$(id).classList.toggle('stale-panel',stale);$(id).title=stale?'Historical evidence; current results are unavailable':'';}
  studyPlots(publication);
  configuration(sheet,select,commit,preview);
  $('execution').textContent=`Execution: ${sheet.assessment.execution} · Feasibility: ${sheet.assessment.feasibility} · Evidence: ${sheet.assessment.evidence} · ${sheet.snapshot.executed.length} nodes updated · ${sheet.snapshot.model_digest.slice(0,12)} · ${sheet.panels.filter(p=>p.status==='approximate').length} approximate analyses`;
}

function requirements(sheet,select){const node=$('requirements');node.replaceChildren();for(const id of sheet.attention){const margin=sheet.margins[id],requirement=sheet.model.requirements[id],row=element('div',undefined,'requirement-row '+margin.status);row.append(element('span',undefined,'dot'));const label=traceButton((margin.provisional?'≈ ':'')+requirement.name,'requirement:'+id,select);label.title=requirement.name+(margin.provisional?' · approximate evidence':'');row.append(label);const value=margin.absolute;row.append(element('span',value?`${value.value>0?'+':''}${quantity(value,2)}`:margin.status,'margin'));node.append(row);}if(!sheet.attention.length)empty(node,'No requirements declared');}

function statements(sheet,mass,operations,select){const node=$('statement');node.replaceChildren();if(mass){const groups={};for(const item of Object.values(mass.details.current.items))groups[item.kind]=(groups[item.kind]||0)+item.mass_kg;for(const [kind,value] of Object.entries(groups))node.append(metric(kind.toUpperCase(),number(value,0),'analysis:'+mass.analysis,select,'kg'));node.append(metric('Loaded mass',number(mass.details.current.total_mass_kg,0),'analysis:'+mass.analysis,select,'kg'));}
  if(operations){
    const report=operations.details,scenarios=Object.keys(report.scenarios||{}),choice=element('select'),details=element('div');
    choice.setAttribute('aria-label','Economic scenario');for(const id of scenarios){const option=element('option',id);option.value=id;choice.append(option);}choice.value=report.scenario;
    const update=()=>{const result=report.scenarios[choice.value]||report;details.replaceChildren();
      details.append(metric('Trip cost',number(result.trip_cost_usd,0),'run:'+operations.run_id,select,'USD'));
      if(result.casm_usd_per_nmi!=null)details.append(metric('CASM',number(result.casm_usd_per_nmi,4),'run:'+operations.run_id,select,'USD/(seat·nmi)'));
      if(result.cost_per_block_hour_usd!=null)details.append(metric('Cost / block hour',number(result.cost_per_block_hour_usd,2),'run:'+operations.run_id,select,'USD/h'));
      if(result.seats===0)details.append(element('div','Non-passenger operation; seat metrics not applicable','muted'));
      if(result.annual_flights)details.append(metric('Annual flights',number(result.annual_flights,0),'run:'+operations.run_id,select));
      details.append(element('div',result.basis?`${result.basis.currency} ${result.basis.price_year} · ${result.basis.cost_scope}`:'Price basis unavailable','muted'));
      const network=metric('All required airports',report.network_compatible?'Compatible':'INCOMPATIBLE','run:'+operations.run_id,select);if(!report.network_compatible)network.classList.add('FAIL');details.append(network);
    };if(scenarios.length)node.append(choice);node.append(details);choice.onchange=update;update();
  }
  if(!mass&&!operations)empty(node,'Mass and operations analyses are unavailable');
}

function plots(geometry,mission,select){const node=$('plots');node.replaceChildren();const plots=[];for(const [label,points,xLabel,yLabel] of [
  ['Envelope area A(x)',geometry?.details.area_distribution?.map(p=>({x:p.station_m,y:p.envelope_area_m2,id:'analysis:'+geometry.analysis})),'station (m)','area (m²)'],
  ['Mission mass',mission?.details.stages?.map(p=>({x:p.end.state.time.value/3600,y:p.end.mass.total_mass_kg,id:'analysis:'+p.analysis,label:p.id})),'time (h)','mass (kg)']]){
    if(!points?.length)continue;const block=element('div',undefined,'plot-block'),title=element('div',label,'plot-label'),frame=element('div',undefined,'plot-frame');block.append(title,frame);node.append(block);plots.push({frame,points,options:{xLabel,yLabel,select,connect:label!=='Envelope area A(x)'}});}
  for(const plot of plots)chart(plot.frame,plot.points,plot.options);
  if(!node.childNodes.length)empty(node,'Plots appear when geometry or mission analysis is available');
}

function massProperties(mass,select,focus){const node=$('mass');node.replaceChildren();if(!mass){empty(node,'Mass analysis is unavailable');return;}const report=mass.details,points=[{x:report.current.cg_m[0],y:report.current.total_mass_kg,label:'Current loading',id:'analysis:'+mass.analysis}];
  for(const [id,case_] of Object.entries(report.loadings))if(case_.state)points.push({x:case_.state.cg_m[0],y:case_.state.total_mass_kg,label:id,id:'loading:'+id});
  const frame=element('div',undefined,'mass-chart');node.append(frame);chart(frame,points,{xLabel:'CG x (m)',yLabel:'mass (kg)',select,connect:false});
  const controls=element('div',undefined,'loading-detail'),choice=element('select');choice.setAttribute('aria-label','Loading case');for(const name of [null,...Object.keys(report.loadings)]){const option=element('option',name===null?'Current loading':name);option.value=JSON.stringify(name);choice.append(option);}const detail=element('span');controls.append(choice,detail);node.append(controls);
  if(focus?.loading?.analysis===mass.analysis&&focus.loading.run_id===mass.run_id)choice.value=JSON.stringify(focus.loading.case);
  const update=()=>{const id=JSON.parse(choice.value),state=id===null?report.current:report.loadings[id].state;detail.textContent=state?`CG [${state.cg_m.map(v=>number(v,2)).join(', ')}] m · I ${state.inertia_kg_m2?'available':'unknown'}`:report.loadings[id].failure;};choice.onchange=update;update();controls.ondblclick=()=>{const id=JSON.parse(choice.value);select(id===null?'analysis:'+mass.analysis:'loading:'+id);};
  if(focus?.loading?.analysis===mass.analysis)node.append(element('div','Limiting loading · '+focus.name,'muted'));

}

function criticalSections(sheet,geometry,select,focus){const node=$('sections');node.replaceChildren();if(!geometry){empty(node,'No geometric section evidence');return;}
  const preferred=(focus?.sections||[]).filter(item=>item.analysis===geometry.analysis&&item.run_id===geometry.run_id).map(item=>item.section);
  const sections=[...geometry.details.sections];sections.sort((a,b)=>{const rank=id=>preferred.includes(id)?preferred.indexOf(id):-1;return (rank(a.id)<0?Infinity:rank(a.id))-(rank(b.id)<0?Infinity:rank(b.id));});
  if(preferred.length)node.append(element('div','Constraint sections · '+focus.name,'muted'));
  for(const section of sections.slice(0,4)){const item=element('div',undefined,'section-item'),text=element('div');const target=section.clearance?'clearance:'+section.clearance:sheet.model.sections[section.id]?'section:'+section.id:'run:'+geometry.run_id;text.append(traceButton(section.id,target,select),element('div',section.reason,'muted'),element('div',`${['x','y','z'][section.axis]} = ${number(section.station_m,3)} m`,'muted'));item.append(sectionSvg(section,geometry.details,select),text);node.append(item);}
}

function preferredFocus(sheet,kind){for(const id of sheet.attention){const focus=sheet.focus[id];if(focus&&(kind==='loading'?focus.loading:focus.sections.length))return {...focus,name:sheet.model.requirements[id].name};}return null;}
export function focusConstraint(sheet,id,select){if(!sheet)return;const mass=panel(sheet,'vehicle.mass_properties'),geometry=panel(sheet,'vehicle.geometry_envelopes');
  if(id.startsWith('requirement:')){const name=id.slice(12),focus=sheet.focus[name];if(!focus)return;const selection={...focus,name:sheet.model.requirements[name].name};if(focus.loading)massProperties(mass,select,selection);if(focus.sections.length)criticalSections(sheet,geometry,select,selection);}
  if(id.startsWith('loading:')&&mass&&Object.hasOwn(mass.details.loadings,id.slice(8))){massProperties(mass,select);const choice=$('mass').querySelector('select');choice.value=JSON.stringify(id.slice(8));choice.onchange();}
}

function closures(sheet,select){const node=$('closure');node.replaceChildren();for(const [id,result] of Object.entries(sheet.snapshot.closures)){const row=element('div',undefined,'closure-row');row.append(traceButton(id,'closure:'+id,select),element('span',`${result.converged?'CLOSED':'FAILED'} · ${result.iterations} iterations`,result.converged?'PASS':'FAIL'));node.append(row);}const pending=Object.values(sheet.snapshot.values).filter(v=>!['input','computed','approximate'].includes(v.status));if(pending.length)node.append(element('div',`${pending.length} unavailable / stale values`,'muted'));
  const failures=sheet.attention.filter(id=>sheet.margins[id].status==='FAIL');for(const id of failures.slice(0,3)){const row=element('div',undefined,'limiting');row.append(traceButton(sheet.model.requirements[id].name,'requirement:'+id,select),element('span',' · '+quantity(sheet.margins[id].absolute)));node.append(row);}if(!failures.length)node.append(element('div',sheet.attention.some(id=>['WARN','UNKNOWN','NOT_APPLICABLE'].includes(sheet.margins[id].status))?'Review active margins and provisional evidence':'Declared requirements satisfied','muted'));
}

export function configuration(sheet,select,commit,preview){const node=$('configuration'),query=$('parameter-search').value.toLowerCase();node.replaceChildren();let keys=Object.keys(sheet.model.parameters).filter(id=>id.toLowerCase().includes(query));if(!query){const preferred=new Set(sheet.model.design_variables.map(v=>v.parameter));keys.sort((a,b)=>Number(preferred.has(b))-Number(preferred.has(a))||Number(sheet.editable.includes(b))-Number(sheet.editable.includes(a))||a.localeCompare(b));}const total=keys.length;if(!query)keys=keys.slice(0,16);for(const id of keys)node.append(inputControl(sheet,id,commit,preview,select));if(!query&&total>16)node.append(element('p',`16 of ${total} parameters · use search to find any input or result`,'muted'));}
