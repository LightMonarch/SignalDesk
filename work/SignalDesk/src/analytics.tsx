import {useState} from 'react';
import {Download} from 'lucide-react';
import {api,errorText,tsNow,type Config,type History,type Metric} from './api';
import {metricNames,MetricChart} from './chart';
import {Button,Notice,PageTitle,Select} from './components/ui';
import {exportData} from './reports';
const groups:Record<string,Metric[]>={'System utilization':['cpu','gpu','memory'],'Temperature':['cpuTemp','gpuTemp','driveTemp'],'Storage throughput':['diskRead','diskWrite'],'Storage usage':['disk'],'Network activity':['rx','tx'],'Battery':['battery','batteryHealth'],'Cooling':['fan'],'Health score':['score'],'Processes':['processes']};
export function Analytics({config:c}:{config:Config}){
 const [group,setGroup]=useState('System utilization'),[error,setError]=useState('');
 return <><PageTitle title="Performance Analytics" description="Compare local historical readings and inspect trends." actions={<><Select label="Metric group" value={group} onChange={setGroup} options={Object.keys(groups)}/><Button onClick={()=>void (async()=>{setError('');try{const end=tsNow()+1;const p=await api<History>('history',{start:end-86400,end});await exportData('signaldesk-performance-24h',{metrics:Object.fromEntries(groups[group].map(k=>[k,metricNames[k]])),points:p.map(x=>({...x,values:Object.fromEntries(Object.entries(x.values).filter(([k])=>groups[group].includes(k as Metric)))}))},'CSV')}catch(e){setError(errorText(e))}})()}><Download size={15}/> Export 24h CSV</Button></>}/>
 {error&&<Notice bad>{error}</Notice>}<MetricChart config={c} keys={groups[group]} title={group} subtitle="Grouped samples preserve minimum, maximum and weighted average." initial={3600}/></>;
}
