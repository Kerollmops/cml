# cml
Coroutines memory lookups - Permit multi memory lookups inside coroutines.

[![CppCon 2018: G. Nishanov "Nano-coroutines to the Rescue! (Using Coroutines TS, of Course)"](https://img.youtube.com/vi/j9tlJAqMV7U/0.jpg)](https://www.youtube.com/watch?v=j9tlJAqMV7U)

```
test tests::basic_300_256mb ... bench:      84,853 ns/iter (+/- 18,628)
test tests::basic_one_256mb ... bench:          21 ns/iter (+/- 2)
test tests::gen_300_256mb   ... bench:      57,564 ns/iter (+/- 17,995)
test tests::gen_one_256mb   ... bench:          65 ns/iter (+/- 7)
```
