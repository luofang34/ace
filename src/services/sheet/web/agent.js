// WebMCP tools (W3C Web Machine Learning Community Group draft): the page exposes the same
// session operations its buttons use, so a browser agent drives the design through typed
// calls instead of screen scraping. Browsers without the API are unaffected.
const NAME_PATTERN=/^[A-Za-z0-9_.-]{1,128}$/;

export function agentTools({request,publication}){
  const current=()=>{const state=publication();if(!state)throw new Error('The design has not loaded yet.');return state;};
  const object=(properties,required=[])=>({type:'object',properties,required,additionalProperties:false});
  const margin=(sheet,id)=>({requirement:id,name:sheet.model.requirements[id]?.name,status:sheet.margins[id]?.status,margin:sheet.margins[id]?.absolute??null,provisional:sheet.margins[id]?.provisional??false});
  return [
    {name:'ace.describe_design',title:'Describe the design',description:'Identity, revision, execution and feasibility assessment, activity and signed requirement margins of the vehicle synthesis sheet.',inputSchema:object({}),annotations:{readOnlyHint:true},
      execute:async()=>{const state=current(),sheet=state.sheet;return {id:sheet.model.id,name:sheet.model.name,revision:sheet.revision,activity:state.activity,assessment:sheet.assessment,saved:state.saved_digest===sheet.snapshot.model_digest,requirements:sheet.attention.map(id=>margin(sheet,id))};}},
    {name:'ace.list_parameters',title:'List editable parameters',description:'Canonical parameters that accept edits, with their current value, unit, source and declared design-variable bounds. Optional case-insensitive substring filter.',inputSchema:object({query:{type:'string',description:'Substring of the parameter name.'}}),annotations:{readOnlyHint:true},
      execute:async({query=''}={})=>{const sheet=current().sheet,needle=String(query).toLowerCase();return sheet.editable.filter(name=>name.toLowerCase().includes(needle)).map(name=>{const variable=sheet.model.design_variables.find(v=>v.parameter===name);return {name,unit:sheet.model.parameters[name]?.unit,value:sheet.snapshot.values[name]?.quantity??null,status:sheet.snapshot.values[name]?.status,source:sheet.model.parameters[name]?.source,lower:variable?.lower??null,upper:variable?.upper??null};});}},
    {name:'ace.read_values',title:'Read current values',description:'Current canonical values and validity status for named parameters or results.',inputSchema:object({names:{type:'array',items:{type:'string'},minItems:1}},['names']),annotations:{readOnlyHint:true},
      execute:async({names})=>{const sheet=current().sheet;return names.map(name=>({name,value:sheet.snapshot.values[name]?.quantity??null,status:sheet.snapshot.values[name]?.status??'unknown'}));}},
    {name:'ace.edit_parameters',title:'Edit parameters',description:'Change canonical input parameters with explicit units. Only affected analyses recompute; the edit is rejected if the design revision moved.',inputSchema:object({updates:{type:'object',minProperties:1,additionalProperties:object({value:{type:'number'},unit:{type:'string'}},['value','unit'])}},['updates']),annotations:{consequentialHint:true},
      execute:async({updates})=>{const revision=current().sheet.revision;await request('/api/edit',{expected_revision:revision,updates});return {accepted:true,base_revision:revision,parameters:Object.keys(updates)};}},
    {name:'ace.run_analysis',title:'Run the analysis graph',description:'Run every stale analysis of the current revision in the formal solver graph.',inputSchema:object({}),annotations:{consequentialHint:true},
      execute:async()=>{const revision=current().sheet.revision;await request('/api/run',{expected_revision:revision});return {accepted:true,base_revision:revision};}},
  ];
}

export function validateTools(tools){
  const names=new Set();
  for(const tool of tools){
    if(!NAME_PATTERN.test(tool.name)||names.has(tool.name))throw new Error('invalid or duplicate tool name: '+tool.name);
    if(typeof tool.description!=='string'||!tool.description)throw new Error(tool.name+': description required');
    if(typeof tool.execute!=='function')throw new Error(tool.name+': execute required');
    if(tool.inputSchema?.type!=='object')throw new Error(tool.name+': object input schema required');
    names.add(tool.name);
  }
  return tools;
}

// `document.modelContext` is the specified surface; early Chrome previews exposed
// `navigator.modelContext` and a `provideContext` batch call.
export function modelContext(scope=globalThis){return scope.document?.modelContext??scope.navigator?.modelContext??null;}

export async function registerAgentTools(tools,scope=globalThis){
  const context=modelContext(scope);
  if(!context)return 'unavailable';
  validateTools(tools);
  if(typeof context.registerTool==='function'){for(const tool of tools)await context.registerTool(tool);return 'registered';}
  if(typeof context.provideContext==='function'){await context.provideContext({tools});return 'provided';}
  return 'unsupported';
}
