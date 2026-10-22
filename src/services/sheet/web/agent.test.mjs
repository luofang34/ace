import test from 'node:test';
import assert from 'node:assert/strict';
import {agentTools,validateTools,registerAgentTools,modelContext} from './agent.js';

const sheet={revision:'r7',model:{id:'concept',name:'Concept',requirements:{span:{name:'Span gate'}},parameters:{'wing.span':{unit:'m',source:{kind:'estimated'}}},design_variables:[{parameter:'wing.span',lower:{value:30,unit:'m'},upper:{value:40,unit:'m'}}]},
  snapshot:{model_digest:'d',values:{'wing.span':{quantity:{value:35,unit:'m'},status:'input'},'weight.mtow':{quantity:{value:70000,unit:'kg'},status:'computed'}}},
  margins:{span:{status:'WARN',absolute:{value:-1,unit:'m'},provisional:false}},attention:['span'],editable:['wing.span'],assessment:{execution:'complete',feasibility:'provisional',evidence:'approximate'}};
const publication={sheet,activity:'ready',saved_digest:'d'};

function harness(){const calls=[];return {calls,tools:agentTools({request:async(path,body)=>{calls.push({path,body});return {};},publication:()=>publication})};}

test('the catalog is a valid WebMCP tool set',()=>{
  const {tools}=harness();
  assert.deepEqual(validateTools(tools).map(t=>t.name),['ace.describe_design','ace.list_parameters','ace.read_values','ace.edit_parameters','ace.run_analysis']);
  assert.ok(tools.every(t=>t.inputSchema.additionalProperties===false));
  assert.ok(tools.filter(t=>t.annotations.readOnlyHint).length===3);
  assert.ok(tools.filter(t=>t.annotations.consequentialHint).length===2);
  assert.throws(()=>validateTools([{name:'bad name',description:'x',execute(){},inputSchema:{type:'object'}}]));
  assert.throws(()=>validateTools([tools[0],tools[0]]));
});

test('read-only tools project the publication without requests',async()=>{
  const {calls,tools}=harness();
  const describe=await tools[0].execute({});
  assert.equal(describe.revision,'r7');
  assert.equal(describe.saved,true);
  assert.deepEqual(describe.requirements,[{requirement:'span',name:'Span gate',status:'WARN',margin:{value:-1,unit:'m'},provisional:false}]);
  const listed=await tools[1].execute({query:'SPAN'});
  assert.equal(listed.length,1);
  assert.deepEqual(listed[0].lower,{value:30,unit:'m'});
  assert.equal((await tools[1].execute({query:'fuel'})).length,0);
  const values=await tools[2].execute({names:['weight.mtow','missing']});
  assert.deepEqual(values,[{name:'weight.mtow',value:{value:70000,unit:'kg'},status:'computed'},{name:'missing',value:null,status:'unknown'}]);
  assert.equal(calls.length,0);
});

test('consequential tools post to the session API guarded by the current revision',async()=>{
  const {calls,tools}=harness();
  const edited=await tools[3].execute({updates:{'wing.span':{value:37,unit:'m'}}});
  assert.deepEqual(edited,{accepted:true,base_revision:'r7',parameters:['wing.span']});
  const ran=await tools[4].execute({});
  assert.equal(ran.base_revision,'r7');
  assert.deepEqual(calls,[{path:'/api/edit',body:{expected_revision:'r7',updates:{'wing.span':{value:37,unit:'m'}}}},{path:'/api/run',body:{expected_revision:'r7'}}]);
});

test('tools are unavailable before the design loads',async()=>{
  const tools=agentTools({request:async()=>{},publication:()=>null});
  await assert.rejects(tools[0].execute({}),/not loaded/);
});

test('registration follows the specified surface, the preview surface, or nothing',async()=>{
  const {tools}=harness();
  const registered=[];
  assert.equal(await registerAgentTools(tools,{document:{modelContext:{registerTool:async t=>{registered.push(t.name);}}}}),'registered');
  assert.equal(registered.length,5);
  let provided=null;
  assert.equal(await registerAgentTools(tools,{navigator:{modelContext:{provideContext:async batch=>{provided=batch;}}}}),'provided');
  assert.equal(provided.tools.length,5);
  assert.equal(await registerAgentTools(tools,{document:{},navigator:{}}),'unavailable');
  assert.equal(await registerAgentTools(tools,{document:{modelContext:{}}}),'unsupported');
  assert.equal(modelContext({document:{modelContext:'a'},navigator:{modelContext:'b'}}),'a');
});
