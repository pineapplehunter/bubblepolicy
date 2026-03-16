# Test cases
These should be used as test ceses

## 1

```
/:ro
/aaa:ro
/aaa/bbb:ro
/aaa/ccc:ro
```

to

```
/:ro
```

## 2

```
/:ro
/aaa:ro
/aaa/bbb:ro
/aaa/ccc:rw
```

to

```
/:ro
/aaa/ccc:rw
```

## 3

```
/:deny
/aaa:ro
/aaa/bbb:tmp
/aaa/ccc/ddd:rw
```

to

```
/aaa:ro
/aaa/bbb:tmp
/aaa/ccc/ddd:rw
```

## 4

```
/:deny
/a:deny
/a/b:ro
/a/b/c:ro
```

to

```
/:deny
/a/b:ro
```
