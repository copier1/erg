from collections.abc import Iterable, Sequence, Iterator, Container
from typing import TypeVar, Union, _SpecialForm, _type_check

class Error:
    def __init__(self, message):
        self.message = message

T = TypeVar("T")
@_SpecialForm
def Result(self, parameters):
    """Result type.

    Result[T] is equivalent to Union[T, Error].
    """
    arg = _type_check(parameters, f"{self} requires a single type.")
    return Union[arg, Error]

def is_ok(obj: Result[T]) -> bool:
    return not isinstance(obj, Error)

def in_operator(x, y):
    if type(y) == type:
        if isinstance(x, y):
            return True
        elif is_ok(y.try_new(x)):
            return True
        # TODO: trait check
        return False
    elif (type(y) == list or type(y) == set) and type(y[0]) == type:
        # FIXME:
        type_check = in_operator(x[0], y[0])
        len_check = len(x) == len(y)
        return type_check and len_check
    elif type(y) == dict and type(y[0]) == type:
        NotImplemented
    else:
        return x in y

class Nat(int):
    def try_new(i: int): # -> Result[Nat]
        if i >= 0:
            return Nat(i)
        else:
            return Error("Nat can't be negative")

    def times(self, f):
        for _ in range(self):
            f()

class Bool(Nat):
    def try_new(b: bool): # -> Result[Nat]
        if b == True or b == False:
            return Bool(b)
        else:
            return Error("Bool can't be other than True or False")

    def __str__(self) -> str:
        if self:
            return "True"
        else:
            return "False"
    def __repr__(self) -> str:
        return self.__str__()

class Range:
    def __init__(self, start, end):
        self.start = start
        self.end = end
    def __contains__(self, item):
        pass
    def __getitem__(self, item):
        pass
    def __len__(self):
        pass
    def __iter__(self):
        return RangeIterator(rng=self)

Sequence.register(Range)
Container.register(Range)
Iterable.register(Range)

# represents `start<..end`
class LeftOpenRange(Range):
    def __contains__(self, item):
        return self.start < item <= self.end
    def __getitem__(self, item):
        return NotImplemented
    def __len__(self):
        return NotImplemented

# represents `start..<end`
class RightOpenRange(Range):
    def __contains__(self, item):
        return self.start <= item < self.end
    def __getitem__(self, item):
        return NotImplemented
    def __len__(self):
        return NotImplemented

# represents `start<..<end`
class OpenRange(Range):
    def __contains__(self, item):
        return self.start < item < self.end
    def __getitem__(self, item):
        return NotImplemented
    def __len__(self):
        return NotImplemented

# represents `start..end`
class ClosedRange(Range):
    def __contains__(self, item):
        return self.start <= item <= self.end
    def __getitem__(self, item):
        return NotImplemented
    def __len__(self):
        return NotImplemented

class RangeIterator:
    def __init__(self, rng):
        self.rng = rng
        self.needle = self.rng.start
        if type(self.rng.start) == int:
            if not(self.needle in self.rng):
                self.needle += 1
        elif type(self.rng.start) == str:
            if not(self.needle in self.rng):
                self.needle = chr(ord(self.needle) + 1)
        else:
            if not(self.needle in self.rng):
                self.needle = self.needle.incremented()

    def __iter__(self):
        return self

    def __next__(self):
        if type(self.rng.start) == int:
            if self.needle in self.rng:
                result = self.needle
                self.needle += 1
                return result
        elif type(self.rng.start) == str:
            if self.needle in self.rng:
                result = self.needle
                self.needle = chr(ord(self.needle) + 1)
                return result
        else:
            if self.needle in self.rng:
                result = self.needle
                self.needle = self.needle.incremented()
                return result
        raise StopIteration

Iterator.register(RangeIterator)
