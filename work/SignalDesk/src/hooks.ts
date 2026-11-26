import {useCallback,useEffect,useRef,useState} from 'react';
import {listen} from '@tauri-apps/api/event';
import {isTauri} from '@tauri-apps/api/core';
import {api,errorText,tsNow,type Config,type History,type State} from './api';
export function useLoad<T>(cmd:string, initial:T){
 const [value,setValue]=useState<T>(initial);
 const [error,setError]=useState('');const [busy,setBusy]=useState(false);const seq=useRef(0);
 const load=useCallback(async(args?:Record<string,unknown>)=>{
  const id=++seq.current;setBusy(true);setError('');
  try{const data=await api<T>(cmd,args);if(seq.current===id)setValue(data??initial);return data}
  catch(e){if(seq.current===id)setError(errorText(e));return undefined}
  finally{if(seq.current===id)setBusy(false)}
 },[cmd]);
 useEffect(()=>()=>{++seq.current},[]);
 return {value,setValue,error,busy,load};
}
export function useSnapshot(){
 const service=useLoad<State>('snapshot',{sample:null,paused:false,error:null,settingsError:null});
 useEffect(()=>{
  if(!isTauri())return;
  let live=true;let ignore:undefined|(()=>void);let unpause:undefined|(()=>void);
  const refresh=()=>{if(live&&document.visibilityState==='visible')void service.load()};
  void listen('sample-ready',refresh).then(fn=>{if(live)ignore=fn;else fn()});
  void listen('paused-changed',refresh).then(fn=>{if(live)unpause=fn;else fn()});
  document.addEventListener('visibilitychange',refresh);refresh();
  return ()=>{live=false;ignore?.();unpause?.();document.removeEventListener('visibilitychange',refresh)};
 },[service.load]);
 return service;
}
export function useHistory(range:number,refresh:number,paused=false,start?:number,end?:number){
 const history=useLoad<History>('history',[]);
 const pull=useCallback(()=>{
  if(document.visibilityState!=='visible'||!isTauri())return;
  const now=tsNow();if(range===0&&(!start||!end||end<=start))return;
  void history.load({start:range===0?start:now-range,end:range===0?end:now+1});
 },[range,start,end,history.load]);
 useEffect(()=>{pull();if(paused||range===0)return;const id=window.setInterval(pull,Math.max(refresh,1)*1000);return()=>window.clearInterval(id)},[pull,paused,range,refresh]);
 return history;
}
export function useTheme(c:Config){
 useEffect(()=>{const media=matchMedia('(prefers-color-scheme: light)');const apply=()=>{document.documentElement.dataset.theme=c.theme==='system'?(media.matches?'light':'dark'):c.theme;document.documentElement.dataset.accent=c.accent};apply();media.addEventListener('change',apply);return()=>media.removeEventListener('change',apply)},[c.theme,c.accent]);
}
