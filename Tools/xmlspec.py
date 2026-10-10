import xml.etree.ElementTree as ET, re, sys, os, json, glob, collections
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from robust import parse_params
R="/Library/Arturia/Jup-8 V4/resources/"
def load(fn):
    txt=open(R+fn,encoding='utf-8').read().replace('&','&amp;').replace('&amp;amp;','&amp;')
    try: return ET.fromstring(txt)
    except ET.ParseError as e:
        print('skip',fn,e); return ET.Element('rootnode')
root=load("Jup-8 V4.xml")
lists={}
def items_of(node):
    out=[]
    for it in node:
        if it.tag=='item':
            if 'text' in it.attrib: out.append(it.attrib['text'])
            elif 'from' in it.attrib:
                a=int(it.attrib['from'])
                if 'to' in it.attrib: out+= [str(v) for v in range(a,int(it.attrib['to'])+1)]
                else:
                    st=int(it.attrib.get('step',1)); out+=[str(a+st*i) for i in range(int(it.attrib['nb']))]
        elif it.tag=='item_list':
            if it.attrib.get('version') and it.attrib['version']!=max(x.attrib.get('version','0') for x in node if x.tag=='item_list'): continue
            out+=lists.get(it.attrib['name'],[])
    return out
for il in root.iter('item_list'):
    if 'name' in il.attrib and len(list(il)):
        lists[il.attrib['name']]=items_of(il)
specs={}
def mapping_of(p):
    ms=[m for m in p if m.tag=='mapping']
    if ms: m=ms[-1].attrib; return m.get('mapping','linear'),m.get('mapping-min'),m.get('mapping-max'),[x.attrib for x in ms]
    return p.attrib.get('mapping','linear'),p.attrib.get('mapping-min'),p.attrib.get('mapping-max'),[]
def walk(node, prefixes):
    for ch in node:
        if ch.tag=='block':
            nm=ch.attrib.get('name',''); nbs=ch.attrib.get('nbblock','1'); nb=int(nbs) if nbs.isdigit() else 4; us=ch.attrib.get('underscore','1')!='0'
            newp=[]
            for p in prefixes:
                if not nm: newp.append(p)
                elif nb>1: newp+= [p+nm+str(i)+'_' for i in range(1,nb+1)]
                else: newp.append(p+nm+('_' if us else ''))
            walk(ch,newp)
        elif ch.tag=='param':
            its=items_of(ch)
            mp=mapping_of(ch)
            for p in prefixes:
                specs[p+ch.attrib['name']]=dict(display=ch.attrib.get('display_name',ch.attrib['name']),items=its,mapping=mp[0],min=mp[1],max=mp[2],versions=mp[3],unit=ch.attrib.get('unit',''),
                    defnorm=ch.attrib.get('defaultvalnorm'),defmapped=ch.attrib.get('defaultvalmapped'),defitem=ch.attrib.get('default_item'))
        elif ch.tag=='xmlfile_parameters':
            walk(load(ch.attrib['filename']),prefixes)
        elif ch.tag in ('vst','internal'):
            walk(ch,prefixes)
walk(root,[''])
for fn in ["vc8_common_vst_params-generated.xml","vc8_common_internal_params-generated.xml"]:
    walk(load(fn),[''])
# corpus majority defaults
files=sorted(glob.glob("/Library/Arturia/Presets/Jup-8 V4/Factory/*/*"))
vals=collections.defaultdict(list)
for f in files:
    for k,v in parse_params(open(f,'rb').read())[1]: vals[k].append(round(v,8))
major={k:collections.Counter(v).most_common(1)[0][0] for k,v in vals.items()}
corpus=set(vals)
missing=[k for k in corpus if k not in specs]
print('specs',len(specs),'corpus',len(corpus),'corpus-not-in-spec',len(missing))
print(sorted(missing)[:80])
json.dump(dict(specs=specs,major=major,order=sorted(corpus)),open(os.path.join(os.path.dirname(os.path.abspath(__file__)),'spec.json'),'w'),indent=0)
