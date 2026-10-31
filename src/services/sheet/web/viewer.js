// Kernel geometry viewer: a Wavefront OBJ artifact rendered with plain WebGL and orbit
// controls. Vehicle axes are x aft, y right, z up; the mesh is drawn exactly as exported.
export function parseObj(text){
  const vertices=[],triangles=[];
  for(const raw of text.split('\n')){
    const line=raw.trim();if(!line||line[0]==='#')continue;
    const parts=line.split(/\s+/);
    if(parts[0]==='v'&&parts.length>=4)vertices.push([+parts[1],+parts[2],+parts[3]]);
    else if(parts[0]==='f'&&parts.length>=4){
      const indices=parts.slice(1).map(token=>{const index=parseInt(token.split('/')[0],10);return index<0?vertices.length+index:index-1;});
      for(let i=1;i+1<indices.length;i++)triangles.push([indices[0],indices[i],indices[i+1]]);
    }
  }
  if(!vertices.length||!triangles.length)throw new Error('OBJ contains no triangles');
  const minimum=[Infinity,Infinity,Infinity],maximum=[-Infinity,-Infinity,-Infinity];
  for(const vertex of vertices)for(let axis=0;axis<3;axis++){if(!Number.isFinite(vertex[axis]))throw new Error('OBJ contains a non-finite vertex');minimum[axis]=Math.min(minimum[axis],vertex[axis]);maximum[axis]=Math.max(maximum[axis],vertex[axis]);}
  const positions=new Float32Array(triangles.length*9),normals=new Float32Array(triangles.length*9);
  let offset=0;
  for(const [a,b,c] of triangles){
    const p=vertices[a],q=vertices[b],r=vertices[c];if(!p||!q||!r)throw new Error('OBJ face references a missing vertex');
    const u=[q[0]-p[0],q[1]-p[1],q[2]-p[2]],w=[r[0]-p[0],r[1]-p[1],r[2]-p[2]];
    const n=[u[1]*w[2]-u[2]*w[1],u[2]*w[0]-u[0]*w[2],u[0]*w[1]-u[1]*w[0]],length=Math.hypot(n[0],n[1],n[2])||1;
    for(const point of [p,q,r]){positions.set(point,offset);normals.set([n[0]/length,n[1]/length,n[2]/length],offset);offset+=3;}
  }
  return {positions,normals,triangles:triangles.length,vertices:vertices.length,minimum,maximum};
}

export function perspective(fovy,aspect,near,far){const f=1/Math.tan(fovy/2),d=near-far;return [f/aspect,0,0,0,0,f,0,0,0,0,(far+near)/d,-1,0,0,2*far*near/d,0];}
export function lookAt(eye,target,up){
  const normalize=v=>{const l=Math.hypot(v[0],v[1],v[2])||1;return [v[0]/l,v[1]/l,v[2]/l];},cross=(a,b)=>[a[1]*b[2]-a[2]*b[1],a[2]*b[0]-a[0]*b[2],a[0]*b[1]-a[1]*b[0]],dot=(a,b)=>a[0]*b[0]+a[1]*b[1]+a[2]*b[2];
  const z=normalize([eye[0]-target[0],eye[1]-target[1],eye[2]-target[2]]),x=normalize(cross(up,z)),y=cross(z,x);
  return [x[0],y[0],z[0],0,x[1],y[1],z[1],0,x[2],y[2],z[2],0,-dot(x,eye),-dot(y,eye),-dot(z,eye),1];
}
export function orbitEye(center,distance,yaw,pitch){return [center[0]+distance*Math.cos(pitch)*Math.cos(yaw),center[1]+distance*Math.cos(pitch)*Math.sin(yaw),center[2]+distance*Math.sin(pitch)];}

const VERTEX=`attribute vec3 position;attribute vec3 normal;uniform mat4 projection;uniform mat4 view;varying vec3 vNormal;varying vec3 vPosition;void main(){vNormal=normal;vPosition=position;gl_Position=projection*view*vec4(position,1.0);}`;
const FRAGMENT=`precision mediump float;varying vec3 vNormal;varying vec3 vPosition;uniform vec3 eye;uniform vec3 color;void main(){vec3 n=normalize(vNormal);vec3 l=normalize(eye-vPosition);float d=abs(dot(n,l));gl_FragColor=vec4(color*(0.3+0.7*d),1.0);}`;
const HOME={yaw:-2.2,pitch:0.45};

export function mountViewer(container,mesh){
  const canvas=document.createElement('canvas');canvas.className='kernel-view';canvas.setAttribute('aria-label','Kernel geometry, drag to orbit, scroll to zoom, double-click to reset');container.append(canvas);
  const gl=canvas.getContext('webgl2',{antialias:true})||canvas.getContext('webgl',{antialias:true});
  if(!gl){canvas.replaceWith(Object.assign(document.createElement('div'),{className:'empty',textContent:'WebGL is unavailable in this browser'}));return {dispose(){}};}
  const program=gl.createProgram();
  for(const [type,source] of [[gl.VERTEX_SHADER,VERTEX],[gl.FRAGMENT_SHADER,FRAGMENT]]){const shader=gl.createShader(type);gl.shaderSource(shader,source);gl.compileShader(shader);gl.attachShader(program,shader);}
  gl.linkProgram(program);gl.useProgram(program);
  const buffer=(name,data)=>{const handle=gl.createBuffer();gl.bindBuffer(gl.ARRAY_BUFFER,handle);gl.bufferData(gl.ARRAY_BUFFER,data,gl.STATIC_DRAW);const location=gl.getAttribLocation(program,name);gl.enableVertexAttribArray(location);gl.vertexAttribPointer(location,3,gl.FLOAT,false,0,0);};
  buffer('position',mesh.positions);buffer('normal',mesh.normals);
  const uniform=name=>gl.getUniformLocation(program,name);
  const center=mesh.minimum.map((low,axis)=>(low+mesh.maximum[axis])/2),radius=Math.hypot(...mesh.maximum.map((high,axis)=>high-mesh.minimum[axis]))/2||1;
  const state={yaw:HOME.yaw,pitch:HOME.pitch,distance:radius*1.9};
  gl.enable(gl.DEPTH_TEST);gl.clearColor(0.965,0.973,0.973,1);
  let pending=false;
  const frame=()=>{pending=false;const ratio=window.devicePixelRatio||1,width=Math.max(1,Math.round(canvas.clientWidth*ratio)),height=Math.max(1,Math.round(canvas.clientHeight*ratio));
    if(canvas.width!==width||canvas.height!==height){canvas.width=width;canvas.height=height;}
    gl.viewport(0,0,width,height);gl.clear(gl.COLOR_BUFFER_BIT|gl.DEPTH_BUFFER_BIT);
    const eye=orbitEye(center,state.distance,state.yaw,state.pitch);
    gl.uniformMatrix4fv(uniform('projection'),false,perspective(0.6,width/height,radius*0.02,radius*20));
    gl.uniformMatrix4fv(uniform('view'),false,lookAt(eye,center,[0,0,1]));
    gl.uniform3fv(uniform('eye'),eye);gl.uniform3fv(uniform('color'),[0.33,0.52,0.62]);
    gl.drawArrays(gl.TRIANGLES,0,mesh.triangles*3);};
  const redraw=()=>{if(!pending){pending=true;requestAnimationFrame(frame);}};
  let drag=null;
  canvas.onpointerdown=event=>{drag={x:event.clientX,y:event.clientY};canvas.setPointerCapture(event.pointerId);};
  canvas.onpointermove=event=>{if(!drag)return;state.yaw-=(event.clientX-drag.x)*0.01;state.pitch=Math.max(-1.5,Math.min(1.5,state.pitch+(event.clientY-drag.y)*0.01));drag={x:event.clientX,y:event.clientY};redraw();};
  canvas.onpointerup=canvas.onpointercancel=()=>{drag=null;};
  canvas.onwheel=event=>{event.preventDefault();state.distance=Math.max(radius*0.3,Math.min(radius*12,state.distance*Math.exp(event.deltaY*0.001)));redraw();};
  canvas.ondblclick=()=>{Object.assign(state,HOME,{distance:radius*1.9});redraw();};
  const observer=new ResizeObserver(redraw);observer.observe(canvas);redraw();
  return {dispose(){observer.disconnect();gl.getExtension('WEBGL_lose_context')?.loseContext();canvas.remove();},redraw};
}
