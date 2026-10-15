import {$,element,number,traceButton,empty} from './controls.js';
import {chart} from './drawing.js';
let selected=null;

export function missionStates(sheet,unused,select,loading){
  const node=$('mission');node.replaceChildren();
  const missions=sheet.panels.filter(p=>p.plugin==='vehicle.mission_sequence');
  const cases=missions.flatMap(panel=>[{key:panel.analysis,report:panel.details,panel},...Object.entries(panel.details.branches||{}).map(([id,report])=>({key:panel.analysis+'/'+id,report,panel}))]);
  if(!cases.length){empty(node,'Mission analysis is unavailable');return;}
  const choice=element('select');choice.setAttribute('aria-label','Mission scenario');
  for(const item of cases){const option=element('option',item.report.mission);option.value=item.key;choice.append(option);}
  choice.value=cases.some(item=>item.key===selected)?selected:cases[0].key;
  const grid=element('div',undefined,'stage-grid');node.append(choice,grid);
  const update=()=>{selected=choice.value;const item=cases.find(item=>item.key===selected);grid.replaceChildren();
    grid.classList.toggle('stale-panel',item.panel.historical);
    for(const stage of item.report.stages){const cell=element('div',undefined,'stage'+(stage.reserve?' reserve':''));cell.append(traceButton(stage.id,'run:'+item.panel.run_id,select));
      const table=element('dl');for(const [label,value] of [['Mass',number(stage.end.mass.total_mass_kg,0)+' kg'],['Fuel burn',number(stage.fuel_burn_kg,0)+' kg'],['Elapsed',number(stage.end.state.time.value/60,1)+' min'],['CG x',number(stage.end.mass.cg_m[0],2)+' m'],['Distance',number(stage.end.state.distance.value/1852,0)+' nmi']])table.append(element('dt',label),element('dd',value));cell.append(table);grid.append(cell);}
    if(!item.report.completed)grid.append(element('div',item.report.failed_stage+': '+item.report.failure,'FAIL'));
    trajectory(sheet,item,select,loading);
  };choice.onchange=update;update();
}

function trajectory(sheet,item,select,loading){
  const node=$('mass');node.replaceChildren();
  const control=element('div',undefined,'loading-detail'),button=element('button','Loading cases');button.onclick=loading;
  control.append(element('span',item.report.mission+' · sampled CG trajectory'),button);node.append(control);
  const envelope=sheet.panels.find(p=>p.plugin==='vehicle.mission_cg_envelope'&&p.details.mission===item.panel.details.mission&&!p.historical);
  const states=[{point:item.report.initial,label:'initial'},...item.report.stages.flatMap(stage=>[...stage.trajectory,stage.end].map((point,index)=>({point,label:stage.id+' #'+index})))];
  const points=states.map(({point,label})=>({x:point.mass.cg_m[0],y:point.mass.total_mass_kg,id:'run:'+item.panel.run_id,label:`${item.report.mission} / ${label} · ${number(point.state.time.value)} s · CG ${number(point.mass.cg_m[0],3)} m · ${number(point.mass.total_mass_kg,1)} kg`}));
  const limits=visibleEnvelope(envelope?.details.envelope||[],points);
  const lines=['minimum_m','maximum_m'].map(bound=>({color:'#a2874e',points:limits.map(limit=>({x:limit[bound],y:limit.mass_kg}))}));
  const frame=element('div',undefined,'mass-chart');node.append(frame);
  chart(frame,points,{xLabel:'CG x (m)',yLabel:'mass (kg)',select,lines});
  const limiting=envelope?.details.limiting;
  node.append(element('div',envelope?.details.failure|| (limiting?`Minimum sampled margin ${number(limiting.margin_m,3)} m · ${limiting.scenario}/${limiting.stage}`:'CG envelope unavailable'),'muted'));
  node.classList.toggle('stale-panel',item.panel.historical);
}

function visibleEnvelope(stations,points){
  const masses=points.map(point=>point.y),low=Math.min(...masses),high=Math.max(...masses),padding=Math.max((high-low)*.1,100);
  const result=[];
  for(let index=1;index<stations.length;index++){
    const a=stations[index-1],b=stations[index],start=Math.max(a.mass_kg,low-padding),end=Math.min(b.mass_kg,high+padding);
    if(start>end)continue;
    for(const mass of [start,end]){const fraction=(mass-a.mass_kg)/(b.mass_kg-a.mass_kg);result.push({mass_kg:mass,minimum_m:a.minimum_m+fraction*(b.minimum_m-a.minimum_m),maximum_m:a.maximum_m+fraction*(b.maximum_m-a.maximum_m)});}
  }
  return result;
}
