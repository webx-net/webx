((globalThis) => {
	globalThis.webx = {
		log: (...args) => {
			Deno.core.print(`[out]: ${argsToMessage(...args)}\n`, false);
		},
		error: (...args) => {
			Deno.core.print(`[err]: ${argsToMessage(...args)}\n`, true);
		},
		static: (path) => Deno.readTextFileSync(path)
	};
})(globalThis);
