export const $ = id => document.getElementById(id);
export function element(tag, text, className) { const node=document.createElement(tag);if(text!==undefined)node.textContent=text;if(className)node.className=className;return node; }
export function number(value, digits=2) { return Number.isFinite(value)?new Intl.NumberFormat('en-US',{maximumFractionDigits:digits}).format(value):'—'; }
export function quantity(q,digits=2) { return q?`${number(q.value,digits)} ${q.unit==='1'?'':q.unit}`.trim():'—'; }
export function traceButton(label,id,select) { const button=element('button',label,'link');button.onclick=()=>select(id);return button; }
export function metric(label,value,id,select,unit='') { const row=element('div',undefined,'metric');row.append(traceButton(label,id,select));const output=element('span',value,'value');if(unit)output.append(element('span',unit,'unit'));row.append(output);return row; }
export function badge(status) { return element('span',String(status).toUpperCase(),'badge '+status); }
export function empty(node,message) { node.replaceChildren(element('div',message,'empty')); }
export function current(sheet,id) { return sheet.snapshot.values[id]?.quantity; }
export function panel(sheet,plugin) { return sheet.panels.find(panel=>panel.plugin===plugin); }
export function inputControl(sheet,name,commit,preview,select,slider=true) {
  const declaration=sheet.model.parameters[name],value=current(sheet,name),row=element('div',undefined,'input-row');
  const label=element('div',undefined,'input-label');label.append(traceButton(name,'parameter:'+name,select));
  if(!sheet.editable.includes(name))label.append(badge(sheet.snapshot.values[name]?.status||'unavailable'));row.append(label);
  if(!sheet.editable.includes(name)){row.append(element('div',quantity(value),'muted'));return row;}
  const controls=element('div',undefined,'input-controls'),field=element('input');field.type='number';field.step='any';field.value=value?.value??'';field.setAttribute('aria-label',name);controls.append(field,element('span',declaration.unit,'unit'));
  const apply=element('button','Apply');apply.onclick=()=>{const number=field.valueAsNumber;if(Number.isFinite(number))commit({[name]:{value:number,unit:declaration.unit}},sheet.revision);else field.reportValidity();};field.required=true;field.onkeydown=event=>{if(event.key==='Enter')apply.click();};controls.append(apply);row.append(controls);
  const variable=sheet.model.design_variables.find(variable=>variable.parameter===name);
  if(slider&&variable&&variable.lower.unit===declaration.unit&&variable.upper.unit===declaration.unit){
    const range=element('input');range.type='range';range.min=variable.lower.value;range.max=variable.upper.value;range.step=(variable.upper.value-variable.lower.value)/200;range.value=value?.value??range.min;range.setAttribute('aria-label',name+' preview');
    range.oninput=()=>{field.value=range.value;preview({[name]:{value:range.valueAsNumber,unit:declaration.unit}},sheet.revision);};
    range.onchange=()=>commit({[name]:{value:range.valueAsNumber,unit:declaration.unit}},sheet.revision);row.append(range);
    const labels=element('div',undefined,'range-labels');labels.append(element('span',quantity(variable.lower)),element('span',quantity(variable.upper)));row.append(labels);
  }
  return row;
}
