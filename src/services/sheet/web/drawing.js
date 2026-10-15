import {element,number,empty} from './controls.js';
const NS='http://www.w3.org/2000/svg';
const colors={wing:'#5c91ab',pressure_vessel:'#90a2a9',fuel_tank:'#cda54f',landing_gear:'#bb7770',structure:'#8d80a6',cabin:'#88b496',cargo_bay:'#b99366',engine:'#a07d94'};
export function svgNode(tag,attrs={},text){const node=document.createElementNS(NS,tag);for(const [name,value] of Object.entries(attrs))node.setAttribute(name,String(value));if(text!==undefined)node.textContent=text;return node;}
function boxPlot(rectangles,width,height,padding=15){
  const bounds=rectangles.flatMap(rect=>[rect.minimum_m,rect.maximum_m]);
  const low=[0,1].map(axis=>Math.min(...bounds.map(point=>point[axis]))),high=[0,1].map(axis=>Math.max(...bounds.map(point=>point[axis])));
  const scale=Math.min((width-2*padding)/Math.max(high[0]-low[0],.01),(height-2*padding)/Math.max(high[1]-low[1],.01));
  const offset=[(width-(high[0]-low[0])*scale)/2,(height-(high[1]-low[1])*scale)/2];
  return {point:p=>[offset[0]+(p[0]-low[0])*scale,offset[1]+(p[1]-low[1])*scale],worldX:x=>(x-offset[0])/scale+low[0],scale};
}
function drawRectangle(svg,rect,transform,select){
  const a=transform.point(rect.minimum_m),b=transform.point(rect.maximum_m);
  const node=svgNode('rect',{x:Math.min(a[0],b[0]),y:Math.min(a[1],b[1]),width:Math.max(Math.abs(b[0]-a[0]),1),height:Math.max(Math.abs(b[1]-a[1]),1),fill:colors[rect.role]||'#8c9da5','fill-opacity':.16,stroke:colors[rect.role]||'#8c9da5','stroke-width':1.2,'data-geometry':rect.geometry,class:'envelope'});
  node.append(svgNode('title',{},rect.geometry));node.onclick=()=>select('geometry:'+rect.geometry);node.ondblclick=()=>select('geometry:'+rect.geometry,true);svg.append(node);return node;
}
export function arrangement(node,report,sheet,select,preview,commit){
  if(!report?.envelopes||!Object.keys(report.envelopes).length){empty(node,'Geometry analysis is unavailable. Select a geometry plugin in the model.');return;}
  node.replaceChildren();const grid=element('div',undefined,'arrangement-grid');node.append(grid);
  for(const [name,axes,sign,width,height] of [['plan',[1,0],1,600,265],['side',[0,2],-1,380,100],['front',[1,2],-1,380,100]]){
    const frame=element('div',undefined,'view-frame view-'+name);frame.append(element('span',name.toUpperCase()));const svg=svgNode('svg',{viewBox:`0 0 ${width} ${height}`,role:'img','aria-label':name+' arrangement'});frame.append(svg);grid.append(frame);
    const rectangles=Object.entries(report.envelopes).map(([id,item])=>({geometry:id,role:item.role,minimum_m:[item.minimum_m[axes[0]],sign*item.minimum_m[axes[1]]],maximum_m:[item.maximum_m[axes[0]],sign*item.maximum_m[axes[1]]]}));
    for(const rectangle of rectangles){const a=rectangle.minimum_m[1],b=rectangle.maximum_m[1];rectangle.minimum_m[1]=Math.min(a,b);rectangle.maximum_m[1]=Math.max(a,b);}
    const transform=boxPlot(rectangles,width,height,18);
    for(const rect of rectangles)drawRectangle(svg,rect,transform,select);
    if(name==='plan'&&sheet.editable.includes('wing.span')){
      const wing=rectangles.find(rect=>rect.role==='wing');if(wing)wingtip(svg,wing,transform,sheet,preview,commit);
    }
    const span=rectangles.flatMap(rect=>[rect.minimum_m[0],rect.maximum_m[0]]);svg.append(svgNode('text',{x:width-8,y:height-6,'text-anchor':'end',class:'axis-label'},number(Math.max(...span)-Math.min(...span),1)+' m'));
  }
}
function wingtip(svg,wing,transform,sheet,preview,commit){
  const location=transform.point([wing.maximum_m[0],(wing.minimum_m[1]+wing.maximum_m[1])/2]);
  const handle=svgNode('circle',{cx:location[0],cy:location[1],r:5.5,class:'wingtip','aria-label':'Drag wingtip to change span',tabindex:0});handle.append(svgNode('title',{},'Drag to change wing.span'));svg.append(handle);
  let active=false,span=null;
  function update(event){const point=svg.createSVGPoint();point.x=event.clientX;point.y=event.clientY;const matrix=svg.getScreenCTM();if(!matrix)return;const local=point.matrixTransform(matrix.inverse());span=2*Math.abs(transform.worldX(local.x));const variable=sheet.model.design_variables.find(variable=>variable.parameter==='wing.span');if(variable){const factor=variable.lower.unit==='ft'?.3048:1;span=Math.max(variable.lower.value*factor,Math.min(variable.upper.value*factor,span));}span=Math.round(span*100)/100;const x=transform.point([span/2,0])[0];handle.setAttribute('cx',x);preview({'wing.span':{value:span,unit:'m'}},sheet.revision);}
  handle.onpointerdown=event=>{active=true;event.preventDefault();handle.setPointerCapture(event.pointerId);};
  handle.onpointermove=event=>{if(active)update(event);};
  handle.onpointerup=event=>{if(active){active=false;handle.releasePointerCapture(event.pointerId);if(span!==null)commit({'wing.span':{value:span,unit:'m'}},sheet.revision);}};
  handle.onpointercancel=()=>{active=false;handle.setAttribute('cx',location[0]);preview(null);};
}
export function sectionSvg(section,report,select){
  const svg=svgNode('svg',{viewBox:'0 0 130 80',role:'img','aria-label':section.reason});
  if(!section.slices.length){svg.append(svgNode('text',{x:7,y:40,class:'axis-label'},'No intersection'));return svg;}
  const rectangles=section.slices.map(slice=>({...slice,role:report.envelopes[slice.geometry]?.role}));const transform=boxPlot(rectangles,130,80,8);for(const rect of rectangles)drawRectangle(svg,rect,transform,select);return svg;
}
export function chart(node,points,{xLabel,yLabel,color='#377e9b',select,connect=true,lines=[]}={}){
  if(!points.length){empty(node,'No current plot evidence');return;}
  const width=Math.max(220,node.clientWidth),height=Math.max(100,node.clientHeight),pad={l:47,r:15,t:14,b:28};
  const extent=[...points,...lines.flatMap(line=>line.points)];
  let minX=Math.min(...extent.map(p=>p.x)),maxX=Math.max(...extent.map(p=>p.x)),minY=Math.min(...extent.map(p=>p.y)),maxY=Math.max(...extent.map(p=>p.y));
  const dx=Math.max(maxX-minX,Math.abs(minX)*.02,.01),dy=Math.max(maxY-minY,Math.abs(minY)*.02,.01);minX-=dx*.05;maxX+=dx*.05;minY-=dy*.05;maxY+=dy*.05;
  const x=v=>pad.l+(v-minX)/(maxX-minX)*(width-pad.l-pad.r),y=v=>height-pad.b-(v-minY)/(maxY-minY)*(height-pad.t-pad.b);
  const svg=svgNode('svg',{viewBox:`0 0 ${width} ${height}`,class:'plot',role:'img','aria-label':`${yLabel} versus ${xLabel}`});
  for(const fraction of [0,.5,1]){const value=minY+(maxY-minY)*fraction,yy=y(value);svg.append(svgNode('line',{x1:pad.l,x2:width-pad.r,y1:yy,y2:yy,stroke:'#e5ecee'}),svgNode('text',{x:pad.l-5,y:yy+3,'text-anchor':'end',class:'axis-label'},number(value,1)));}
  svg.append(svgNode('text',{x:width-pad.r,y:height-3,'text-anchor':'end',class:'axis-label'},xLabel),svgNode('text',{x:pad.l,y:10,class:'axis-label'},yLabel),svgNode('text',{x:pad.l,y:height-12,class:'axis-label'},number(minX,1)),svgNode('text',{x:width-pad.r,y:height-12,'text-anchor':'end',class:'axis-label'},number(maxX,1)));
  for(const line of lines)svg.append(svgNode('polyline',{points:line.points.map(p=>`${x(p.x)},${y(p.y)}`).join(' '),fill:'none',stroke:line.color||color,'stroke-width':1,'stroke-dasharray':'4 3'}));
  if(connect)svg.append(svgNode('polyline',{points:points.map(p=>`${x(p.x)},${y(p.y)}`).join(' '),fill:'none',stroke:color,'stroke-width':1.8}));
  for(const point of points){const circle=svgNode('circle',{cx:x(point.x),cy:y(point.y),r:connect?2:4,fill:point.color||color,tabindex:point.id?0:-1});circle.append(svgNode('title',{},point.label||`${number(point.x)} / ${number(point.y)}`));if(point.id&&select){circle.style.cursor='pointer';circle.onclick=()=>select(point.id);circle.onkeydown=event=>{if(event.key==='Enter')select(point.id);};}svg.append(circle);}
  node.replaceChildren(svg);
}
