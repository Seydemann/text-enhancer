;;; scratch-magic-polish.el --- Polish *scratch* buffer with Gemini -*- lexical-binding: t; -*-

;; Minimal, non-invasive utility:
;; - One explicit command: `scratch-magic-polish`
;; - Only runs in *scratch*
;; - Sends entire buffer as <raw>...</raw>
;; - Replaces entire buffer with model output

(require 'json)
(require 'subr-x)
(require 'url)

(defgroup scratch-magic nil
  "Polish text in *scratch* using Gemini."
  :group 'tools
  :prefix "scratch-magic-")

(defcustom scratch-magic-model "gemini-3-flash-preview"
  "Gemini model ID used for polish requests."
  :type 'string
  :group 'scratch-magic)

(defcustom scratch-magic-api-key nil
  "Gemini API key. If nil, read from GEMINI_API_KEY environment variable."
  :type '(choice (const :tag "Use GEMINI_API_KEY env var" nil)
                 string)
  :group 'scratch-magic)

(defcustom scratch-magic-timeout-seconds 60
  "HTTP timeout in seconds for Gemini requests."
  :type 'integer
  :group 'scratch-magic)

(defcustom scratch-magic-system-prompt
  (concat
   "Linguistically polish the text in <raw>. Preserve the author's voice, "
   "personality, tone, and style -- elevate, never replace. Find more "
   "idiomatic phrasing where natural, eliminate bloat, and fix grammatical, "
   "lexical, and punctuation errors. Do not restructure, reconstruct, or "
   "re-engineer the content. Every edit should unseal, not substitute. Where "
   "the author reached for a phrase and fell short, complete the arch they "
   "were already building -- reveal what they were on the verge of writing, "
   "never impose what they weren't. The goal is not correction but "
   "emancipation: widen the bottleneck between thought and expression until "
   "what arrives on the page is proportionate to what was luminous in the "
   "mind. Surgical, enriching changes only.")
  "System prompt used for the polish request."
  :type 'string
  :group 'scratch-magic)

(defun scratch-magic--api-key ()
  "Return API key from customization or environment."
  (or scratch-magic-api-key
      (getenv "GEMINI_API_KEY")
      (user-error
       "Set `scratch-magic-api-key` or GEMINI_API_KEY before using scratch-magic")))

(defun scratch-magic--endpoint ()
  "Build Gemini generateContent endpoint URL."
  (format "https://generativelanguage.googleapis.com/v1beta/models/%s:generateContent"
          scratch-magic-model))

(defun scratch-magic--build-request (raw-text)
  "Build JSON payload for RAW-TEXT."
  (let ((wrapped-raw (format "<raw>%s</raw>" raw-text)))
    (json-encode
     (list
      (cons "systemInstruction"
            (list
             (cons "parts"
                   (vector
                    (list
                     (cons "text" scratch-magic-system-prompt))))))
      (cons "contents"
            (vector
             (list
              (cons "parts"
                    (vector
                     (list
                      (cons "text" wrapped-raw)))))))))))

(defun scratch-magic--extract-response-text (response)
  "Extract text from parsed RESPONSE alist."
  (let* ((candidates (alist-get "candidates" response nil nil #'string=))
         (first-candidate (car-safe candidates))
         (content (and first-candidate
                       (alist-get "content" first-candidate nil nil #'string=)))
         (parts (and content (alist-get "parts" content nil nil #'string=)))
         (text-parts
          (delq nil
                (mapcar (lambda (part) (alist-get "text" part nil nil #'string=))
                        parts))))
    (if text-parts
        (string-join text-parts "")
      (user-error "Gemini response did not contain text content"))))

(defun scratch-magic--request (raw-text)
  "Send RAW-TEXT to Gemini and return polished text."
  (let* ((url-request-method "POST")
         (url-request-extra-headers
          `(("Content-Type" . "application/json")
            ("x-goog-api-key" . ,(scratch-magic--api-key))))
         (url-request-data
          (encode-coding-string (scratch-magic--build-request raw-text) 'utf-8))
         (coding-system-for-read 'utf-8-unix)
         (coding-system-for-write 'utf-8-unix)
         (buffer (url-retrieve-synchronously
                  (scratch-magic--endpoint)
                  t
                  t
                  scratch-magic-timeout-seconds)))
    (unless buffer
      (user-error "Gemini request failed: no response"))
    (with-current-buffer buffer
      (unwind-protect
          (progn
            (goto-char (point-min))
            (let ((status
                   (if (re-search-forward "^HTTP/[0-9.]+ \\([0-9]+\\)" nil t)
                       (string-to-number (match-string 1))
                     0)))
              (goto-char (or url-http-end-of-headers (point-min)))
              (when (> (point) (point-max))
                (user-error "Unexpected Gemini response format"))
              (let* ((json-object-type 'alist)
                     (json-array-type 'list)
                     (json-key-type 'string)
                     (response (json-read))
                     (api-error (alist-get "error" response nil nil #'string=)))
                (when api-error
                  (let ((message (or (alist-get "message" api-error nil nil #'string=)
                                     "Unknown API error")))
                    (user-error "Gemini API error (%s): %s" status message)))
                (unless (= status 200)
                  (user-error "Gemini request failed with HTTP status %s" status))
                (scratch-magic--extract-response-text response))))
        (kill-buffer buffer)))))

;;;###autoload
(defun scratch-magic-polish ()
  "Polish the entire *scratch* buffer and replace it with the result."
  (interactive)
  (unless (string= (buffer-name) "*scratch*")
    (user-error "This command only runs in *scratch*"))
  (let ((raw-text (buffer-substring-no-properties (point-min) (point-max))))
    (when (string-empty-p (string-trim raw-text))
      (user-error "*scratch* is empty"))
    (message "Scratch magic: polishing...")
    (let ((polished (scratch-magic--request raw-text)))
      (atomic-change-group
        (let ((inhibit-read-only t))
          (erase-buffer)
          (insert polished)))
      (goto-char (point-min))
      (message "Scratch magic: done"))))

(provide 'scratch-magic-polish)

;;; scratch-magic-polish.el ends here
