;;; init.el --- alan-emacs entrypoint -*- lexical-binding: t; -*-

;;; Commentary:
;; Source-owned vanilla Emacs configuration.

;;; Code:

(setq user-emacs-directory
      (file-name-directory (or load-file-name buffer-file-name)))

(add-to-list 'load-path (expand-file-name "lisp" user-emacs-directory))

(require 'alan-core)
(require 'alan-ui)
(require 'alan-editing)
(require 'alan-project)
(require 'alan-git)

(let ((local-file (expand-file-name "alan-local.el" user-emacs-directory)))
  (when (file-exists-p local-file)
    (load local-file nil t)))

(provide 'init)
;;; init.el ends here
