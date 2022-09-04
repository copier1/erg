# Add R

```erg
Add R = Trait {
    .AddO = Type
    .`_+_` = (Self, R) -> Self.AddO
}
```

`Add`是一种定义加法的类型。加法有两种类型的`+`：方法和函数
`+`作为二元函数，即`_+_`，定义如下：

```erg
`_+_`(l: Add(R, O), r: R): O = l.`_+_` r
```

这个定义的目的是让 `+` 可以被视为一个函数而不是一个方法

```erg
assert [1, 2, 3].fold(0, `_+_`) == 6

call op, x, y = op(x, y)
assert call(`_+_`, 1, 2) == 3
```

加法是这样输入的。

```erg
f: |O: Type; A <: Add(Int, O)| A -> O
f x = x + 1

g: |A, O: Type; Int <: Add(A, O)| A -> O
g x = 1 + x
```