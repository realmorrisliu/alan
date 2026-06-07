;;; alan-core.el --- Core defaults for alan-emacs -*- lexical-binding: t; -*-

;;; Commentary:
;; Core behavior and filesystem locations. This module intentionally sticks to
;; built-in Emacs features.

;;; Code:

(require 'files)
(require 'recentf)
(require 'savehist)
(require 'saveplace)

(defun alan-emacs--xdg-dir (environment-variable fallback)
  "Return ENVIRONMENT-VARIABLE or FALLBACK expanded under `~'."
  (file-name-as-directory
   (expand-file-name (or (getenv environment-variable) fallback))))

(defvar alan-emacs-state-directory
  (expand-file-name "alan-emacs/"
                    (alan-emacs--xdg-dir "XDG_STATE_HOME" "~/.local/state/"))
  "Directory for durable local state.")

(defvar alan-emacs-cache-directory
  (expand-file-name "alan-emacs/"
                    (alan-emacs--xdg-dir "XDG_CACHE_HOME" "~/.cache/"))
  "Directory for disposable cache state.")

(dolist (directory (list alan-emacs-state-directory alan-emacs-cache-directory))
  (make-directory directory t))

(setq custom-file (expand-file-name "custom.el" alan-emacs-state-directory))
(when (file-exists-p custom-file)
  (load custom-file nil t))

(setq savehist-file (expand-file-name "history" alan-emacs-state-directory))
(setq save-place-file (expand-file-name "places" alan-emacs-state-directory))
(setq recentf-save-file (expand-file-name "recentf" alan-emacs-state-directory))
(setq bookmark-default-file (expand-file-name "bookmarks" alan-emacs-state-directory))
(setq project-list-file (expand-file-name "projects" alan-emacs-state-directory))

(setq backup-directory-alist
      `(("." . ,(expand-file-name "backups/" alan-emacs-state-directory))))
(setq auto-save-file-name-transforms
      `((".*" ,(expand-file-name "auto-save/" alan-emacs-cache-directory) t)))
(make-directory (expand-file-name "backups/" alan-emacs-state-directory) t)
(make-directory (expand-file-name "auto-save/" alan-emacs-cache-directory) t)

(setq create-lockfiles nil)
(setq auto-save-default t)
(setq make-backup-files t)
(setq version-control t)
(setq delete-old-versions t)
(setq kept-new-versions 8)
(setq kept-old-versions 2)

(setq use-short-answers t)
(setq ring-bell-function #'ignore)
(setq sentence-end-double-space nil)
(setq require-final-newline t)
(setq confirm-kill-emacs #'y-or-n-p)
(setq history-length 1000)
(setq savehist-additional-variables '(kill-ring search-ring regexp-search-ring))
(setq recentf-max-saved-items 200)
(setq recentf-auto-cleanup 'never)

(setq package-user-dir (expand-file-name "elpa/" alan-emacs-cache-directory))

(savehist-mode 1)
(save-place-mode 1)
(recentf-mode 1)
(global-auto-revert-mode 1)

(when (fboundp 'repeat-mode)
  (repeat-mode 1))

(provide 'alan-core)
;;; alan-core.el ends here
