org 100h
xor ax,ax
jcxz done
again: add ax,cx
loop again
done: hlt
