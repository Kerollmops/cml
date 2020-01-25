# cml
Coroutines memory lookups - Permit multi memory lookups inside coroutines.

[![CppCon 2018: G. Nishanov "Nano-coroutines to the Rescue! (Using Coroutines TS, of Course)"](https://img.youtube.com/vi/j9tlJAqMV7U/0.jpg)](https://www.youtube.com/watch?v=j9tlJAqMV7U)

```
test tests::basic_110_256mb ... bench:      16,369 ns/iter (+/- 3,438)
test tests::basic_one_256mb ... bench:          21 ns/iter (+/- 2)
test tests::gen_110_256mb   ... bench:      15,941 ns/iter (+/- 2,305)
test tests::gen_one_256mb   ... bench:          65 ns/iter (+/- 7)
```
