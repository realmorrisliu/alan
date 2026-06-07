;;; early-init.el --- Early startup for alan-emacs -*- lexical-binding: t; -*-

;;; Commentary:
;; Keep startup predictable before package and UI initialization.

;;; Code:

(setq package-enable-at-startup nil)
(setq frame-inhibit-implied-resize t)
(setq inhibit-startup-screen t)
(setq inhibit-startup-message t)
(setq inhibit-splash-screen t)

(setq gc-cons-threshold most-positive-fixnum)
(add-hook 'emacs-startup-hook
          (lambda ()
            (setq gc-cons-threshold (* 64 1024 1024))))

(push '(tool-bar-lines . 0) default-frame-alist)
(push '(menu-bar-lines . 0) default-frame-alist)
(push '(vertical-scroll-bars . nil) default-frame-alist)

(provide 'early-init)
;;; early-init.el ends here
