import React,{Suspense,useEffect,useRef,useState} from 'react';
import {createRoot} from 'react-dom/client';
import {isTauri} from '@tauri-apps/api/core';
import {Activity,AppWindow,BatteryCharging,Bell,ChartNoAxesCombined,ChevronDown,ChevronsUpDown,Cpu,Database,Fan,FileChartColumn,FlaskConical,Gauge,HardDrive,LayoutDashboard,MemoryStick,Monitor,PanelLeftClose,PanelLeftOpen,Pause,Play,Power,ScanLine,Search,Settings as SettingsIcon,ShieldCheck,Sparkles,Wifi,X} from 'lucide-react';
import '@fontsource-variable/inter';
import './styles.css';
import {api,defaults,errorText,type Config} from './api';
import {useSnapshot,useTheme} from './hooks';
import {Badge,Button,Empty,Notice} from './components/ui';
import {Overview} from './overview';
import {Hardware} from './hardware';
const Operations=React.lazy(()=>import('./operations').then(m=>({default:OperationsPage(m)})));
const Analytics=React.lazy(()=>import('./analytics').then(m=>({default:m.Analytics})));
const Reports=React.lazy(()=>import('./reports').then(m=>({default:m.Reports})));
const Assistant=React.lazy(()=>import('./assistant').then(m=>({default:m.Assistant})));
const Settings=React.lazy(()=>import('./settings').then(m=>({default:m.Settings})));
const nav=[
 {section:'WORKSPACE',items:[['Overview',LayoutDashboard],['System Information',Monitor],['Performance Analytics',ChartNoAxesCombined]]},
 {section:'HARDWARE',items:[['CPU',Cpu],['GPU',HardDrive],['Memory',MemoryStick],['Storage',Database],['Battery',BatteryCharging],['Network',Wifi],['Cooling and Sensors',Fan]]},
 {section:'MANAGEMENT',items:[['Processes',Activity],['Startup Manager',Power],['Diagnostics',ScanLine],['Benchmarks',FlaskConical],['Event Logs',FileChartColumn],['Alerts',Bell],['Reports',FileChartColumn]]},
 {section:'PREFERENCES',items:[['AI Assistant',Sparkles],['Settings',SettingsIcon]]}
] as const;
const names=nav.flatMap(g=>g.items.map(([p])=>p));type Page=typeof names[number];
const hardware=nav[1].items.map(([p])=>p) as readonly string[];
function OperationsPage(m:typeof import('./operations')){return ({page,config,sample}:{page:string;config:Config;sample:ReturnType<typeof useSnapshot>['value']['sample']})=>page==='Processes'?<m.Processes config={config}/>:page==='Startup Manager'?<m.StartupManager/>:page==='Diagnostics'?<m.Issues sample={sample} diagnostics/>:page==='Alerts'?<m.Issues sample={sample}/>:page==='Benchmarks'?<m.Benchmarks config={config} sample={sample}/>:<m.EventLogs/>}
function App(){
 const [page,setPage]=useState<Page>('Overview'),[collapsed,setCollapsed]=useState(()=>localStorage.getItem('sidebar')==='compact'),[query,setQuery]=useState(''),[search,setSearch]=useState(false),[config,setConfig]=useState(defaults),[database,setDatabase]=useState(''),[failure,setFailure]=useState('');
 const monitor=useSnapshot();const input=useRef<HTMLInputElement>(null);const scroller=useRef<HTMLDivElement>(null);
 const matches=names.filter(n=>n.toLowerCase().includes(query.trim().toLowerCase()));
 const {sample:s,paused,error,settingsError}=monitor.value;
 useTheme(config);
 const loadSettings=async()=>{try{const v=await api<{config:Config;database:string}>('settings');setConfig({...defaults,...v.config});setDatabase(v.database)}catch(e){setFailure(errorText(e))}};
 useEffect(()=>{if(isTauri())void loadSettings();const key=(e:KeyboardEvent)=>{if((e.ctrlKey||e.metaKey)&&e.key.toLowerCase()==='k'){e.preventDefault();input.current?.focus()}};window.addEventListener('keydown',key);return()=>window.removeEventListener('keydown',key)},[]);
 useEffect(()=>{scroller.current?.scrollTo(0,0)},[page]);
 const go=(name:string)=>{if(names.includes(name as Page)){setPage(name as Page);setSearch(false);setQuery('');input.current?.blur()}};
 const save=async(c:Config)=>{const v=await api<Config>('save_settings',{config:c});setConfig({...defaults,...v});await loadSettings();void monitor.load();return v};
 const toggle=async()=>{setFailure('');try{await api('pause');void monitor.load()}catch(e){setFailure(errorText(e))}};
 return <div className={'app '+(collapsed?'nav-compact':'')}>
 <aside className="sidebar"><div className="brand"><div className="brand-icon"><Activity size={23} strokeWidth={2.3}/></div>{!collapsed&&<div><b>SignalDesk</b><span>System Health</span></div>}</div><nav aria-label="Main navigation">{nav.map(g=><div key={g.section} className="nav-group">{!collapsed&&<span className="nav-heading">{g.section}</span>}{g.items.map(([label,Icon])=><button key={label} className={'nav-item '+(label===page?'active':'')} title={label} aria-label={label} aria-current={label===page?'page':undefined} onClick={()=>go(label)}><Icon size={17} strokeWidth={1.75}/>{!collapsed&&<span>{label}</span>}</button>)}</div>)}</nav>
 <button title={collapsed?'Expand sidebar':'Collapse sidebar'} aria-label={collapsed?'Expand sidebar':'Collapse sidebar'} className="collapse-nav" onClick={()=>{setCollapsed(!collapsed);localStorage.setItem('sidebar',collapsed?'full':'compact')}}>{collapsed?<PanelLeftOpen size={17}/>:<PanelLeftClose size={17}/>} {!collapsed&&'Collapse sidebar'}</button></aside>
 <div className="workspace"><header className="topbar"><div className="breadcrumb"><AppWindow size={16}/> <span>Workspace</span><ChevronDown size={13} className="crumb-arrow"/><b>{page}</b></div>
 <div className="top-controls"><div className="global-search" onBlur={e=>{if(!e.currentTarget.contains(e.relatedTarget))setSearch(false)}}><Search size={16}/><input ref={input} aria-label="Search pages" placeholder="Go to a page…" value={query} onFocus={()=>setSearch(true)} onChange={e=>{setQuery(e.target.value);setSearch(true)}} onKeyDown={e=>{if(e.key==='Escape'){setSearch(false);e.currentTarget.blur()}if(e.key==='Enter'&&matches[0])go(matches[0])}}/><kbd>Ctrl K</kbd>{search&&<div className="search-results">{matches.length?matches.map(n=><button key={n} onClick={()=>go(n)}>{n}</button>):<span>No matching pages</span>}</div>}</div>
 <Button variant="ghost" className="pause-toggle" title={paused?'Resume monitoring':'Pause monitoring'} aria-label={paused?'Resume monitoring':'Pause monitoring'} onClick={()=>void toggle()}>{paused?<Play size={17}/>:<Pause size={17}/>}</Button><Button variant="ghost" className="top-bell" aria-label="Open alerts" onClick={()=>go('Alerts')}><Bell size={18}/>{!!s?.issues.length&&<i/>}</Button><span className="top-divider"/><div className="device-icon" title={s?.host??'Local computer'}><Monitor size={17}/></div></div></header>
 <main ref={scroller} className="main"><div className="content">
 {(failure||error||monitor.error||settingsError)&&<Notice bad><div className="tools"><span>{failure||error||monitor.error||settingsError}</span>{failure&&<Button aria-label="Dismiss error" variant="ghost" onClick={()=>setFailure('')}><X size={14}/></Button>}</div></Notice>}
 <Suspense fallback={<Empty text="Opening page" loading/>}>{page==='Overview'?<Overview sample={s} config={config} save={save} go={go} paused={paused}/>:page==='System Information'||hardware.includes(page)?<Hardware page={page} sample={s} config={config}/>:page==='Performance Analytics'?<Analytics config={config}/>:page==='Reports'?<Reports sample={s}/>:page==='AI Assistant'?<Assistant config={config} settings={()=>go('Settings')}/>:page==='Settings'?<Settings config={config} save={save} changed={loadSettings} database={database} customize={()=>go('Overview')}/>:<Operations key={page} page={page} sample={s} config={config}/>}</Suspense>
 <footer><span><ShieldCheck size={14}/> Local monitoring</span><span>{s?.os??'Windows desktop'} · SignalDesk 1.0.0</span></footer>
 </div></main></div></div>;
}
createRoot(document.getElementById('root')!).render(<App/>);
