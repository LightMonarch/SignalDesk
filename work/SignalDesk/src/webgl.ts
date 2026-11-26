export async function gpuBenchmark(seconds:number,stop:()=>boolean,progress:(p:number)=>void){
 const canvas=document.createElement('canvas');canvas.width=768;canvas.height=432;
 const gl=canvas.getContext('webgl2',{antialias:false,powerPreference:'high-performance',failIfMajorPerformanceCaveat:true});if(!gl)throw Error('An accelerated WebGL 2 adapter is unavailable.');
 const ext=gl.getExtension('WEBGL_debug_renderer_info');const renderer=String(gl.getParameter(ext?.UNMASKED_RENDERER_WEBGL??gl.RENDERER));
 if(/swiftshader|software|basic render/i.test(renderer))throw Error('WebView2 selected a software renderer.');
 const shader=(type:number,source:string)=>{const s=gl.createShader(type);if(!s)throw Error('GPU shader allocation failed.');gl.shaderSource(s,source);gl.compileShader(s);if(!gl.getShaderParameter(s,gl.COMPILE_STATUS)){const reason=gl.getShaderInfoLog(s);gl.deleteShader(s);throw Error(reason??'GPU shader compilation failed.')}return s};
 let vs:WebGLShader|undefined,fs:WebGLShader|undefined;
 const p=gl.createProgram();
 try{
  vs=shader(gl.VERTEX_SHADER,'#version 300 es\nvoid main(){vec2 v=vec2(float((gl_VertexID<<1)&2),float(gl_VertexID&2));gl_Position=vec4(v*2.0-1.0,0,1);}');
  fs=shader(gl.FRAGMENT_SHADER,'#version 300 es\nprecision highp float;uniform float t;out vec4 pixel;void main(){vec2 p=gl_FragCoord.xy/768.0; vec3 c=vec3(p,t);for(int i=0;i<38;i++){c=sin(c.zxy*vec3(1.7,2.5,3.3)+float(i)*0.31+p.xyx);}pixel=vec4(c*0.5+0.5,1);}');
  gl.attachShader(p,vs);gl.attachShader(p,fs);gl.linkProgram(p);if(!gl.getProgramParameter(p,gl.LINK_STATUS))throw Error(gl.getProgramInfoLog(p)??'GPU shader link failed.');gl.useProgram(p);
  const timer=gl.getUniformLocation(p,'t'),pixel=new Uint8Array(4);
  const started=performance.now();let frames=0;let elapsed=0;
  do{
   if(stop())throw Error('Benchmark stopped by cancellation or temperature cutoff.');
   if(gl.isContextLost())throw Error('The GPU context was lost.');
   for(let i=0;i<3;i++){gl.uniform1f(timer,frames/147);gl.drawArrays(gl.TRIANGLES,0,3);gl.readPixels(381,209,1,1,gl.RGBA,gl.UNSIGNED_BYTE,pixel);frames++}
   if(gl.getError()!==gl.NO_ERROR)throw Error('WebGL reported an error during the workload.');
   elapsed=(performance.now()-started)/1000;progress(Math.min(99,Math.round(elapsed/seconds*100)));await new Promise<void>(r=>setTimeout(r,0));
  }while(elapsed<seconds);
  progress(100);return {fps:frames/elapsed,duration:elapsed,renderer};
 }finally{if(vs)gl.deleteShader(vs);if(fs)gl.deleteShader(fs);gl.deleteProgram(p);gl.getExtension('WEBGL_lose_context')?.loseContext()}
}
