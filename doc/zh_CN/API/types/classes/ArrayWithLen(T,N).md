# ArrayWithLen T: Type, N: Nat

`[T; N]`是语法糖。还有一个[`Array` 类型](./Array.md)省略了长度。

## methods

* values_at(self, selectors: [Nat; N]) -> [T; N]

```erg
assert ["a", "b", "c", "d", "e"].values_at([0, 1, 3]) == ["a", "b", "d"]
```

* all(self, pred: T -> Bool) -> Bool
  返回是否所有元素都满足 pred。
   如果元素为 0，则无论 pred 为 `True`，但会发出警告。
   该规范本身已被多种语言采用，是逻辑一致性所必需的。

  ```erg
  assert [].all(_ -> False)
  ```

  ```python
  assert all(False for _ in [])
  ```

## methods of ArrayWithLen T, N | T <: Eq

* freq self -> [{T: Nat}]
  返回对象出现的次数。

```erg
assert ["a", "b", "c", "b", "c", "b"].freq() \
== [{"a", 1}, {"b": 3}, {"c": 2}]
```