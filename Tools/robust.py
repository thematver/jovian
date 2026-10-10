import re
def parse_params(b):
    """Find the parameter map: '<N> 0 0 0 (<len> <key> <num>)*N <M> 0 (<len> <key> <len> <blob>)*M \\n' consuming to EOF."""
    best=None
    for m in re.finditer(rb'(?:(?<=\s)|^)(\d+) 0 0 0 (?=\d+ )', b):
        n=int(m.group(1)); i=m.end(); out=[]
        try:
            for _ in range(n):
                j=b.index(b' ',i); L=int(b[i:j]); k=b[j+1:j+1+L]; i=j+1+L+1
                j=b.index(b' ',i); v=float(b[i:j]); out.append((k.decode(),v)); i=j+1
            j=b.index(b' ',i); M=int(b[i:j]); i=j+1
            j=b.index(b' ',i); assert b[i:j]==b'0'; i=j+1
            for _ in range(M):
                j=b.index(b' ',i); L=int(b[i:j]); i=j+1+L+1
                j=b.index(b' ',i); L=int(b[i:j]); i=j+1+L+1
            if b[i-1:].strip()==b'' or i>=len(b)-1: best=(m.start(),out)
        except Exception: pass
    return best
