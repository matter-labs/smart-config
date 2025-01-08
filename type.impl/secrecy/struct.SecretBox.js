(function() {var type_impls = {
"smart_config":[["<details class=\"toggle implementors-toggle\" open><summary><section id=\"impl-Clone-for-SecretBox%3CS%3E\" class=\"impl\"><a href=\"#impl-Clone-for-SecretBox%3CS%3E\" class=\"anchor\">§</a><h3 class=\"code-header\">impl&lt;S&gt; <a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/core/clone/trait.Clone.html\" title=\"trait core::clone::Clone\">Clone</a> for SecretBox&lt;S&gt;<div class=\"where\">where\n    S: CloneableSecret,</div></h3></section></summary><div class=\"impl-items\"><details class=\"toggle method-toggle\" open><summary><section id=\"method.clone\" class=\"method trait-impl\"><a href=\"#method.clone\" class=\"anchor\">§</a><h4 class=\"code-header\">fn <a href=\"https://doc.rust-lang.org/nightly/core/clone/trait.Clone.html#tymethod.clone\" class=\"fn\">clone</a>(&amp;self) -&gt; SecretBox&lt;S&gt;</h4></section></summary><div class='docblock'>Returns a copy of the value. <a href=\"https://doc.rust-lang.org/nightly/core/clone/trait.Clone.html#tymethod.clone\">Read more</a></div></details><details class=\"toggle method-toggle\" open><summary><section id=\"method.clone_from\" class=\"method trait-impl\"><span class=\"rightside\"><span class=\"since\" title=\"Stable since Rust version 1.0.0\">1.0.0</span> · <a class=\"src\" href=\"https://doc.rust-lang.org/nightly/src/core/clone.rs.html#175\">source</a></span><a href=\"#method.clone_from\" class=\"anchor\">§</a><h4 class=\"code-header\">fn <a href=\"https://doc.rust-lang.org/nightly/core/clone/trait.Clone.html#method.clone_from\" class=\"fn\">clone_from</a>(&amp;mut self, source: <a class=\"primitive\" href=\"https://doc.rust-lang.org/nightly/std/primitive.reference.html\">&amp;Self</a>)</h4></section></summary><div class='docblock'>Performs copy-assignment from <code>source</code>. <a href=\"https://doc.rust-lang.org/nightly/core/clone/trait.Clone.html#method.clone_from\">Read more</a></div></details></div></details>","Clone","smart_config::value::SecretString"],["<details class=\"toggle implementors-toggle\" open><summary><section id=\"impl-Clone-for-SecretBox%3Cstr%3E\" class=\"impl\"><a href=\"#impl-Clone-for-SecretBox%3Cstr%3E\" class=\"anchor\">§</a><h3 class=\"code-header\">impl <a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/core/clone/trait.Clone.html\" title=\"trait core::clone::Clone\">Clone</a> for SecretBox&lt;<a class=\"primitive\" href=\"https://doc.rust-lang.org/nightly/std/primitive.str.html\">str</a>&gt;</h3></section></summary><div class=\"impl-items\"><details class=\"toggle method-toggle\" open><summary><section id=\"method.clone\" class=\"method trait-impl\"><a href=\"#method.clone\" class=\"anchor\">§</a><h4 class=\"code-header\">fn <a href=\"https://doc.rust-lang.org/nightly/core/clone/trait.Clone.html#tymethod.clone\" class=\"fn\">clone</a>(&amp;self) -&gt; SecretBox&lt;<a class=\"primitive\" href=\"https://doc.rust-lang.org/nightly/std/primitive.str.html\">str</a>&gt;</h4></section></summary><div class='docblock'>Returns a copy of the value. <a href=\"https://doc.rust-lang.org/nightly/core/clone/trait.Clone.html#tymethod.clone\">Read more</a></div></details><details class=\"toggle method-toggle\" open><summary><section id=\"method.clone_from\" class=\"method trait-impl\"><span class=\"rightside\"><span class=\"since\" title=\"Stable since Rust version 1.0.0\">1.0.0</span> · <a class=\"src\" href=\"https://doc.rust-lang.org/nightly/src/core/clone.rs.html#175\">source</a></span><a href=\"#method.clone_from\" class=\"anchor\">§</a><h4 class=\"code-header\">fn <a href=\"https://doc.rust-lang.org/nightly/core/clone/trait.Clone.html#method.clone_from\" class=\"fn\">clone_from</a>(&amp;mut self, source: <a class=\"primitive\" href=\"https://doc.rust-lang.org/nightly/std/primitive.reference.html\">&amp;Self</a>)</h4></section></summary><div class='docblock'>Performs copy-assignment from <code>source</code>. <a href=\"https://doc.rust-lang.org/nightly/core/clone/trait.Clone.html#method.clone_from\">Read more</a></div></details></div></details>","Clone","smart_config::value::SecretString"],["<details class=\"toggle implementors-toggle\" open><summary><section id=\"impl-Debug-for-SecretBox%3CS%3E\" class=\"impl\"><a href=\"#impl-Debug-for-SecretBox%3CS%3E\" class=\"anchor\">§</a><h3 class=\"code-header\">impl&lt;S&gt; <a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/core/fmt/trait.Debug.html\" title=\"trait core::fmt::Debug\">Debug</a> for SecretBox&lt;S&gt;<div class=\"where\">where\n    S: Zeroize + ?<a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/core/marker/trait.Sized.html\" title=\"trait core::marker::Sized\">Sized</a>,</div></h3></section></summary><div class=\"impl-items\"><details class=\"toggle method-toggle\" open><summary><section id=\"method.fmt\" class=\"method trait-impl\"><a href=\"#method.fmt\" class=\"anchor\">§</a><h4 class=\"code-header\">fn <a href=\"https://doc.rust-lang.org/nightly/core/fmt/trait.Debug.html#tymethod.fmt\" class=\"fn\">fmt</a>(&amp;self, f: &amp;mut <a class=\"struct\" href=\"https://doc.rust-lang.org/nightly/core/fmt/struct.Formatter.html\" title=\"struct core::fmt::Formatter\">Formatter</a>&lt;'_&gt;) -&gt; <a class=\"enum\" href=\"https://doc.rust-lang.org/nightly/core/result/enum.Result.html\" title=\"enum core::result::Result\">Result</a>&lt;<a class=\"primitive\" href=\"https://doc.rust-lang.org/nightly/std/primitive.unit.html\">()</a>, <a class=\"struct\" href=\"https://doc.rust-lang.org/nightly/core/fmt/struct.Error.html\" title=\"struct core::fmt::Error\">Error</a>&gt;</h4></section></summary><div class='docblock'>Formats the value using the given formatter. <a href=\"https://doc.rust-lang.org/nightly/core/fmt/trait.Debug.html#tymethod.fmt\">Read more</a></div></details></div></details>","Debug","smart_config::value::SecretString"],["<details class=\"toggle implementors-toggle\" open><summary><section id=\"impl-Default-for-SecretBox%3CS%3E\" class=\"impl\"><a href=\"#impl-Default-for-SecretBox%3CS%3E\" class=\"anchor\">§</a><h3 class=\"code-header\">impl&lt;S&gt; <a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/core/default/trait.Default.html\" title=\"trait core::default::Default\">Default</a> for SecretBox&lt;S&gt;<div class=\"where\">where\n    S: Zeroize + <a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/core/default/trait.Default.html\" title=\"trait core::default::Default\">Default</a>,</div></h3></section></summary><div class=\"impl-items\"><details class=\"toggle method-toggle\" open><summary><section id=\"method.default\" class=\"method trait-impl\"><a href=\"#method.default\" class=\"anchor\">§</a><h4 class=\"code-header\">fn <a href=\"https://doc.rust-lang.org/nightly/core/default/trait.Default.html#tymethod.default\" class=\"fn\">default</a>() -&gt; SecretBox&lt;S&gt;</h4></section></summary><div class='docblock'>Returns the “default value” for a type. <a href=\"https://doc.rust-lang.org/nightly/core/default/trait.Default.html#tymethod.default\">Read more</a></div></details></div></details>","Default","smart_config::value::SecretString"],["<details class=\"toggle implementors-toggle\" open><summary><section id=\"impl-Default-for-SecretBox%3Cstr%3E\" class=\"impl\"><a href=\"#impl-Default-for-SecretBox%3Cstr%3E\" class=\"anchor\">§</a><h3 class=\"code-header\">impl <a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/core/default/trait.Default.html\" title=\"trait core::default::Default\">Default</a> for SecretBox&lt;<a class=\"primitive\" href=\"https://doc.rust-lang.org/nightly/std/primitive.str.html\">str</a>&gt;</h3></section></summary><div class=\"impl-items\"><details class=\"toggle method-toggle\" open><summary><section id=\"method.default\" class=\"method trait-impl\"><a href=\"#method.default\" class=\"anchor\">§</a><h4 class=\"code-header\">fn <a href=\"https://doc.rust-lang.org/nightly/core/default/trait.Default.html#tymethod.default\" class=\"fn\">default</a>() -&gt; SecretBox&lt;<a class=\"primitive\" href=\"https://doc.rust-lang.org/nightly/std/primitive.str.html\">str</a>&gt;</h4></section></summary><div class='docblock'>Returns the “default value” for a type. <a href=\"https://doc.rust-lang.org/nightly/core/default/trait.Default.html#tymethod.default\">Read more</a></div></details></div></details>","Default","smart_config::value::SecretString"],["<details class=\"toggle implementors-toggle\" open><summary><section id=\"impl-Drop-for-SecretBox%3CS%3E\" class=\"impl\"><a href=\"#impl-Drop-for-SecretBox%3CS%3E\" class=\"anchor\">§</a><h3 class=\"code-header\">impl&lt;S&gt; <a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/core/ops/drop/trait.Drop.html\" title=\"trait core::ops::drop::Drop\">Drop</a> for SecretBox&lt;S&gt;<div class=\"where\">where\n    S: Zeroize + ?<a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/core/marker/trait.Sized.html\" title=\"trait core::marker::Sized\">Sized</a>,</div></h3></section></summary><div class=\"impl-items\"><details class=\"toggle method-toggle\" open><summary><section id=\"method.drop\" class=\"method trait-impl\"><a href=\"#method.drop\" class=\"anchor\">§</a><h4 class=\"code-header\">fn <a href=\"https://doc.rust-lang.org/nightly/core/ops/drop/trait.Drop.html#tymethod.drop\" class=\"fn\">drop</a>(&amp;mut self)</h4></section></summary><div class='docblock'>Executes the destructor for this type. <a href=\"https://doc.rust-lang.org/nightly/core/ops/drop/trait.Drop.html#tymethod.drop\">Read more</a></div></details></div></details>","Drop","smart_config::value::SecretString"],["<details class=\"toggle implementors-toggle\" open><summary><section id=\"impl-ExposeSecret%3CS%3E-for-SecretBox%3CS%3E\" class=\"impl\"><a href=\"#impl-ExposeSecret%3CS%3E-for-SecretBox%3CS%3E\" class=\"anchor\">§</a><h3 class=\"code-header\">impl&lt;S&gt; ExposeSecret&lt;S&gt; for SecretBox&lt;S&gt;<div class=\"where\">where\n    S: Zeroize + ?<a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/core/marker/trait.Sized.html\" title=\"trait core::marker::Sized\">Sized</a>,</div></h3></section></summary><div class=\"impl-items\"><details class=\"toggle method-toggle\" open><summary><section id=\"method.expose_secret\" class=\"method trait-impl\"><a href=\"#method.expose_secret\" class=\"anchor\">§</a><h4 class=\"code-header\">fn <a class=\"fn\">expose_secret</a>(&amp;self) -&gt; <a class=\"primitive\" href=\"https://doc.rust-lang.org/nightly/std/primitive.reference.html\">&amp;S</a></h4></section></summary><div class='docblock'>Expose secret: this is the only method providing access to a secret.</div></details></div></details>","ExposeSecret<S>","smart_config::value::SecretString"],["<details class=\"toggle implementors-toggle\" open><summary><section id=\"impl-ExposeSecretMut%3CS%3E-for-SecretBox%3CS%3E\" class=\"impl\"><a href=\"#impl-ExposeSecretMut%3CS%3E-for-SecretBox%3CS%3E\" class=\"anchor\">§</a><h3 class=\"code-header\">impl&lt;S&gt; ExposeSecretMut&lt;S&gt; for SecretBox&lt;S&gt;<div class=\"where\">where\n    S: Zeroize + ?<a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/core/marker/trait.Sized.html\" title=\"trait core::marker::Sized\">Sized</a>,</div></h3></section></summary><div class=\"impl-items\"><details class=\"toggle method-toggle\" open><summary><section id=\"method.expose_secret_mut\" class=\"method trait-impl\"><a href=\"#method.expose_secret_mut\" class=\"anchor\">§</a><h4 class=\"code-header\">fn <a class=\"fn\">expose_secret_mut</a>(&amp;mut self) -&gt; <a class=\"primitive\" href=\"https://doc.rust-lang.org/nightly/std/primitive.reference.html\">&amp;mut S</a></h4></section></summary><div class='docblock'>Expose secret: this is the only method providing access to a secret.</div></details></div></details>","ExposeSecretMut<S>","smart_config::value::SecretString"],["<details class=\"toggle implementors-toggle\" open><summary><section id=\"impl-From%3C%26str%3E-for-SecretBox%3Cstr%3E\" class=\"impl\"><a href=\"#impl-From%3C%26str%3E-for-SecretBox%3Cstr%3E\" class=\"anchor\">§</a><h3 class=\"code-header\">impl <a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/core/convert/trait.From.html\" title=\"trait core::convert::From\">From</a>&lt;&amp;<a class=\"primitive\" href=\"https://doc.rust-lang.org/nightly/std/primitive.str.html\">str</a>&gt; for SecretBox&lt;<a class=\"primitive\" href=\"https://doc.rust-lang.org/nightly/std/primitive.str.html\">str</a>&gt;</h3></section></summary><div class=\"impl-items\"><details class=\"toggle method-toggle\" open><summary><section id=\"method.from\" class=\"method trait-impl\"><a href=\"#method.from\" class=\"anchor\">§</a><h4 class=\"code-header\">fn <a href=\"https://doc.rust-lang.org/nightly/core/convert/trait.From.html#tymethod.from\" class=\"fn\">from</a>(s: &amp;<a class=\"primitive\" href=\"https://doc.rust-lang.org/nightly/std/primitive.str.html\">str</a>) -&gt; SecretBox&lt;<a class=\"primitive\" href=\"https://doc.rust-lang.org/nightly/std/primitive.str.html\">str</a>&gt;</h4></section></summary><div class='docblock'>Converts to this type from the input type.</div></details></div></details>","From<&str>","smart_config::value::SecretString"],["<details class=\"toggle implementors-toggle\" open><summary><section id=\"impl-From%3CBox%3CS%3E%3E-for-SecretBox%3CS%3E\" class=\"impl\"><a href=\"#impl-From%3CBox%3CS%3E%3E-for-SecretBox%3CS%3E\" class=\"anchor\">§</a><h3 class=\"code-header\">impl&lt;S&gt; <a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/core/convert/trait.From.html\" title=\"trait core::convert::From\">From</a>&lt;<a class=\"struct\" href=\"https://doc.rust-lang.org/nightly/alloc/boxed/struct.Box.html\" title=\"struct alloc::boxed::Box\">Box</a>&lt;S&gt;&gt; for SecretBox&lt;S&gt;<div class=\"where\">where\n    S: Zeroize + ?<a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/core/marker/trait.Sized.html\" title=\"trait core::marker::Sized\">Sized</a>,</div></h3></section></summary><div class=\"impl-items\"><details class=\"toggle method-toggle\" open><summary><section id=\"method.from\" class=\"method trait-impl\"><a href=\"#method.from\" class=\"anchor\">§</a><h4 class=\"code-header\">fn <a href=\"https://doc.rust-lang.org/nightly/core/convert/trait.From.html#tymethod.from\" class=\"fn\">from</a>(source: <a class=\"struct\" href=\"https://doc.rust-lang.org/nightly/alloc/boxed/struct.Box.html\" title=\"struct alloc::boxed::Box\">Box</a>&lt;S&gt;) -&gt; SecretBox&lt;S&gt;</h4></section></summary><div class='docblock'>Converts to this type from the input type.</div></details></div></details>","From<Box<S>>","smart_config::value::SecretString"],["<details class=\"toggle implementors-toggle\" open><summary><section id=\"impl-From%3CString%3E-for-SecretBox%3Cstr%3E\" class=\"impl\"><a href=\"#impl-From%3CString%3E-for-SecretBox%3Cstr%3E\" class=\"anchor\">§</a><h3 class=\"code-header\">impl <a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/core/convert/trait.From.html\" title=\"trait core::convert::From\">From</a>&lt;<a class=\"struct\" href=\"https://doc.rust-lang.org/nightly/alloc/string/struct.String.html\" title=\"struct alloc::string::String\">String</a>&gt; for SecretBox&lt;<a class=\"primitive\" href=\"https://doc.rust-lang.org/nightly/std/primitive.str.html\">str</a>&gt;</h3></section></summary><div class=\"impl-items\"><details class=\"toggle method-toggle\" open><summary><section id=\"method.from\" class=\"method trait-impl\"><a href=\"#method.from\" class=\"anchor\">§</a><h4 class=\"code-header\">fn <a href=\"https://doc.rust-lang.org/nightly/core/convert/trait.From.html#tymethod.from\" class=\"fn\">from</a>(s: <a class=\"struct\" href=\"https://doc.rust-lang.org/nightly/alloc/string/struct.String.html\" title=\"struct alloc::string::String\">String</a>) -&gt; SecretBox&lt;<a class=\"primitive\" href=\"https://doc.rust-lang.org/nightly/std/primitive.str.html\">str</a>&gt;</h4></section></summary><div class='docblock'>Converts to this type from the input type.</div></details></div></details>","From<String>","smart_config::value::SecretString"],["<details class=\"toggle implementors-toggle\" open><summary><section id=\"impl-SecretBox%3CS%3E\" class=\"impl\"><a href=\"#impl-SecretBox%3CS%3E\" class=\"anchor\">§</a><h3 class=\"code-header\">impl&lt;S&gt; SecretBox&lt;S&gt;<div class=\"where\">where\n    S: Zeroize + <a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/core/clone/trait.Clone.html\" title=\"trait core::clone::Clone\">Clone</a>,</div></h3></section></summary><div class=\"impl-items\"><details class=\"toggle method-toggle\" open><summary><section id=\"method.init_with\" class=\"method\"><h4 class=\"code-header\">pub fn <a class=\"fn\">init_with</a>(ctr: impl <a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/core/ops/function/trait.FnOnce.html\" title=\"trait core::ops::function::FnOnce\">FnOnce</a>() -&gt; S) -&gt; SecretBox&lt;S&gt;</h4></section></summary><div class=\"docblock\"><p>Create a secret value using the provided function as a constructor.</p>\n<p>The implementation makes an effort to zeroize the locally constructed value\nbefore it is copied to the heap, and constructing it inside the closure minimizes\nthe possibility of it being accidentally copied by other code.</p>\n<p><strong>Note:</strong> using [<code>Self::new</code>] or [<code>Self::init_with_mut</code>] is preferable when possible,\nsince this method’s safety relies on empiric evidence and may be violated on some targets.</p>\n</div></details><details class=\"toggle method-toggle\" open><summary><section id=\"method.try_init_with\" class=\"method\"><h4 class=\"code-header\">pub fn <a class=\"fn\">try_init_with</a>&lt;E&gt;(\n    ctr: impl <a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/core/ops/function/trait.FnOnce.html\" title=\"trait core::ops::function::FnOnce\">FnOnce</a>() -&gt; <a class=\"enum\" href=\"https://doc.rust-lang.org/nightly/core/result/enum.Result.html\" title=\"enum core::result::Result\">Result</a>&lt;S, E&gt;,\n) -&gt; <a class=\"enum\" href=\"https://doc.rust-lang.org/nightly/core/result/enum.Result.html\" title=\"enum core::result::Result\">Result</a>&lt;SecretBox&lt;S&gt;, E&gt;</h4></section></summary><div class=\"docblock\"><p>Same as [<code>Self::init_with</code>], but the constructor can be fallible.</p>\n<p><strong>Note:</strong> using [<code>Self::new</code>] or [<code>Self::init_with_mut</code>] is preferable when possible,\nsince this method’s safety relies on empyric evidence and may be violated on some targets.</p>\n</div></details></div></details>",0,"smart_config::value::SecretString"],["<details class=\"toggle implementors-toggle\" open><summary><section id=\"impl-SecretBox%3CS%3E\" class=\"impl\"><a href=\"#impl-SecretBox%3CS%3E\" class=\"anchor\">§</a><h3 class=\"code-header\">impl&lt;S&gt; SecretBox&lt;S&gt;<div class=\"where\">where\n    S: Zeroize + <a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/core/default/trait.Default.html\" title=\"trait core::default::Default\">Default</a>,</div></h3></section></summary><div class=\"impl-items\"><details class=\"toggle method-toggle\" open><summary><section id=\"method.init_with_mut\" class=\"method\"><h4 class=\"code-header\">pub fn <a class=\"fn\">init_with_mut</a>(ctr: impl <a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/core/ops/function/trait.FnOnce.html\" title=\"trait core::ops::function::FnOnce\">FnOnce</a>(<a class=\"primitive\" href=\"https://doc.rust-lang.org/nightly/std/primitive.reference.html\">&amp;mut S</a>)) -&gt; SecretBox&lt;S&gt;</h4></section></summary><div class=\"docblock\"><p>Create a secret value using a function that can initialize the value in-place.</p>\n</div></details></div></details>",0,"smart_config::value::SecretString"],["<details class=\"toggle implementors-toggle\" open><summary><section id=\"impl-SecretBox%3CS%3E\" class=\"impl\"><a href=\"#impl-SecretBox%3CS%3E\" class=\"anchor\">§</a><h3 class=\"code-header\">impl&lt;S&gt; SecretBox&lt;S&gt;<div class=\"where\">where\n    S: Zeroize + ?<a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/core/marker/trait.Sized.html\" title=\"trait core::marker::Sized\">Sized</a>,</div></h3></section></summary><div class=\"impl-items\"><details class=\"toggle method-toggle\" open><summary><section id=\"method.new\" class=\"method\"><h4 class=\"code-header\">pub fn <a class=\"fn\">new</a>(boxed_secret: <a class=\"struct\" href=\"https://doc.rust-lang.org/nightly/alloc/boxed/struct.Box.html\" title=\"struct alloc::boxed::Box\">Box</a>&lt;S&gt;) -&gt; SecretBox&lt;S&gt;</h4></section></summary><div class=\"docblock\"><p>Create a secret value using a pre-boxed value.</p>\n</div></details></div></details>",0,"smart_config::value::SecretString"],["<details class=\"toggle implementors-toggle\" open><summary><section id=\"impl-Zeroize-for-SecretBox%3CS%3E\" class=\"impl\"><a href=\"#impl-Zeroize-for-SecretBox%3CS%3E\" class=\"anchor\">§</a><h3 class=\"code-header\">impl&lt;S&gt; Zeroize for SecretBox&lt;S&gt;<div class=\"where\">where\n    S: Zeroize + ?<a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/core/marker/trait.Sized.html\" title=\"trait core::marker::Sized\">Sized</a>,</div></h3></section></summary><div class=\"impl-items\"><details class=\"toggle method-toggle\" open><summary><section id=\"method.zeroize\" class=\"method trait-impl\"><a href=\"#method.zeroize\" class=\"anchor\">§</a><h4 class=\"code-header\">fn <a class=\"fn\">zeroize</a>(&amp;mut self)</h4></section></summary><div class='docblock'>Zero out this object from memory using Rust intrinsics which ensure the\nzeroization operation is not “optimized away” by the compiler.</div></details></div></details>","Zeroize","smart_config::value::SecretString"],["<section id=\"impl-ZeroizeOnDrop-for-SecretBox%3CS%3E\" class=\"impl\"><a href=\"#impl-ZeroizeOnDrop-for-SecretBox%3CS%3E\" class=\"anchor\">§</a><h3 class=\"code-header\">impl&lt;S&gt; ZeroizeOnDrop for SecretBox&lt;S&gt;<div class=\"where\">where\n    S: Zeroize + ?<a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/core/marker/trait.Sized.html\" title=\"trait core::marker::Sized\">Sized</a>,</div></h3></section>","ZeroizeOnDrop","smart_config::value::SecretString"]]
};if (window.register_type_impls) {window.register_type_impls(type_impls);} else {window.pending_type_impls = type_impls;}})()