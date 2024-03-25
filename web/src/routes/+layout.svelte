<script lang="ts">
	import '../app.postcss';

	// Highlight JS
	import hljs from 'highlight.js/lib/core';
	import 'highlight.js/styles/github-dark.css';
	import { storeHighlightJs } from '@skeletonlabs/skeleton';
    import diff from 'highlight.js/lib/languages/diff';
    import shell from 'highlight.js/lib/languages/shell';
    import bash from 'highlight.js/lib/languages/bash';
    import json from 'highlight.js/lib/languages/json';

    hljs.registerLanguage('diff', diff);
    hljs.registerLanguage('shell', shell);
    hljs.registerLanguage('bash', bash);
    hljs.registerLanguage('json', json);
	storeHighlightJs.set(hljs);
</script>

<slot />
