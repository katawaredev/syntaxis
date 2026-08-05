(() => {
	const mounted = new WeakSet();
	const mountedEditors = new WeakSet();
	const draftKeys = new WeakMap();
	const recognitions = new Map();

	const mount = (container) => {
		if (mounted.has(container)) return;
		mounted.add(container);
		let stickToEnd = true;

		container.addEventListener(
			"scroll",
			() => {
				stickToEnd =
					container.scrollHeight -
						container.scrollTop -
						container.clientHeight <
					96;
			},
			{ passive: true },
		);

		new MutationObserver(() => {
			if (stickToEnd) container.scrollTop = container.scrollHeight;
		}).observe(container, {
			childList: true,
			subtree: true,
			characterData: true,
		});

		container.scrollTop = container.scrollHeight;
	};

	const discover = () => {
		document.querySelectorAll("[data-agent-scroll]").forEach(mount);
		document.querySelectorAll(".ai-composer-input").forEach(mountEditor);
	};

	const mountEditor = (input) => {
		if (mountedEditors.has(input)) {
			input.syntaxisSyncDraft?.();
			input.syntaxisSyncHeight?.();
			return;
		}
		mountedEditors.add(input);
		const editor = input.closest(".ai-composer-editor");
		if (!editor) return;

		const sync = () => {
			input.style.height = "auto";
			const height = Math.max(64, Math.min(192, input.scrollHeight));
			input.style.height = `${height}px`;
			editor.style.height = `${height}px`;
		};

		const syncDraft = () => {
			const key = input.dataset.draftKey;
			if (!key || draftKeys.get(input) === key) return;
			draftKeys.set(input, key);
			try {
				const saved = window.localStorage?.getItem(key) ?? "";
				if (input.value !== saved) {
					input.value = saved;
					input.dispatchEvent(new Event("input", { bubbles: true }));
				}
			} catch {
				// Storage can be unavailable in private or locked-down browsers.
			}
		};

		input.syntaxisSyncHeight = sync;
		input.syntaxisSyncDraft = syncDraft;
		input.addEventListener("input", () => {
			sync();
			const key = input.dataset.draftKey;
			if (!key) return;
			try {
				if (input.value) window.localStorage?.setItem(key, input.value);
				else window.localStorage?.removeItem(key);
			} catch {
				// Draft persistence is a convenience, never a reason to block typing.
			}
		});
		input.addEventListener("keydown", (event) => {
			// Software keyboards have no practical Shift+Enter gesture. Keep Return
			// available for multiline prompts; the adjacent Send button submits.
			if (
				window.matchMedia("(pointer: coarse)").matches &&
				event.key === "Enter"
			) {
				event.stopPropagation();
			}
		});
		input.addEventListener("paste", async (event) => {
			const clipboardFiles = Array.from(event.clipboardData?.files ?? []);
			const itemFiles = Array.from(event.clipboardData?.items ?? [])
				.filter((item) => item.kind === "file")
				.map((item) => item.getAsFile())
				.filter(Boolean);
			const pastedFiles = clipboardFiles.length ? clipboardFiles : itemFiles;
			const images = pastedFiles.filter((file) =>
				file.type.startsWith("image/"),
			);
			if (!images.length) {
				if (pastedFiles.length) {
					event.preventDefault();
					emitPaste(input.id, {
						kind: "error",
						message: "Pi accepts image attachments only.",
					});
				}
				return;
			}
			event.preventDefault();

			if (input.dataset.imagesEnabled !== "true") {
				emitPaste(input.id, {
					kind: "error",
					message: "The selected model does not accept images.",
				});
				return;
			}

			for (const image of images) {
				if (image.size > 8 * 1024 * 1024) {
					emitPaste(input.id, {
						kind: "error",
						message: "Images can be 8 MiB each and 16 MiB total.",
					});
					continue;
				}
				try {
					const dataUrl = await readAsDataUrl(image);
					emitPaste(input.id, {
						kind: "image",
						name: image.name || "Pasted image",
						mime_type: image.type,
						data: dataUrl.slice(dataUrl.indexOf(",") + 1),
					});
				} catch {
					emitPaste(input.id, {
						kind: "error",
						message: "Could not read the pasted image.",
					});
				}
			}
		});
		syncDraft();
		requestAnimationFrame(sync);
	};

	new MutationObserver(discover).observe(document.documentElement, {
		childList: true,
		subtree: true,
		attributes: true,
		attributeFilter: ["data-draft-key"],
	});
	discover();

	const emitPaste = (id, detail) => {
		window.dispatchEvent(
			new CustomEvent("syntaxis-ai-paste", {
				detail: { id, ...detail },
			}),
		);
	};

	const readAsDataUrl = (file) =>
		new Promise((resolve, reject) => {
			const reader = new FileReader();
			reader.onload = () => resolve(String(reader.result));
			reader.onerror = reject;
			reader.readAsDataURL(file);
		});

	const emitSpeech = (id, detail) => {
		window.dispatchEvent(
			new CustomEvent("syntaxis-ai-speech", {
				detail: { id, ...detail },
			}),
		);
	};

	const speechError = (error) => {
		if (error === "not-allowed" || error === "service-not-allowed") {
			return "Microphone access was denied. Allow it in your browser settings to use dictation.";
		}
		if (error === "no-speech")
			return "No speech was detected. Try again when you are ready.";
		if (error === "audio-capture") return "No microphone is available.";
		return "Speech recognition stopped unexpectedly.";
	};

	const toggleSpeech = (id) => {
		const active = recognitions.get(id);
		if (active) {
			active.stop();
			return;
		}

		const SpeechRecognition =
			window.SpeechRecognition || window.webkitSpeechRecognition;
		if (!SpeechRecognition) {
			emitSpeech(id, {
				kind: "error",
				message: "Speech recognition is not supported by this browser.",
			});
			return;
		}

		const recognition = new SpeechRecognition();
		recognition.continuous = true;
		recognition.interimResults = false;
		recognition.lang =
			document.documentElement.lang || navigator.language || "en-US";
		recognition.onstart = () => emitSpeech(id, { kind: "start" });
		recognition.onresult = (event) => {
			let text = "";
			for (
				let index = event.resultIndex;
				index < event.results.length;
				index += 1
			) {
				if (event.results[index].isFinal)
					text += event.results[index][0].transcript;
			}
			if (text.trim()) emitSpeech(id, { kind: "transcript", text });
		};
		recognition.onerror = (event) => {
			if (event.error !== "aborted") {
				emitSpeech(id, { kind: "error", message: speechError(event.error) });
			}
		};
		recognition.onend = () => {
			recognitions.delete(id);
			emitSpeech(id, { kind: "end" });
		};
		recognitions.set(id, recognition);
		try {
			recognition.start();
		} catch (error) {
			recognitions.delete(id);
			emitSpeech(id, {
				kind: "error",
				message: String(error?.message ?? error),
			});
		}
	};

	window.SyntaxisAiChat = { toggleSpeech };
})();
