;;; alan-editing.el --- Editing defaults for alan-emacs -*- lexical-binding: t; -*-

;;; Commentary:
;; Small editing improvements that keep vanilla Emacs recognizable.

;;; Code:

(setq-default indent-tabs-mode nil)
(setq-default tab-width 4)
(setq-default fill-column 100)

(setq tab-always-indent 'complete)
(setq save-interprogram-paste-before-kill t)
(setq kill-do-not-save-duplicates t)

(delete-selection-mode 1)
(electric-pair-mode 1)
(show-paren-mode 1)

(when (fboundp 'global-so-long-mode)
  (global-so-long-mode 1))

(add-hook 'before-save-hook #'delete-trailing-whitespace)
(add-hook 'text-mode-hook #'visual-line-mode)

(global-set-key (kbd "C-c r") #'revert-buffer)
(global-set-key (kbd "C-c w") #'whitespace-mode)

(provide 'alan-editing)
;;; alan-editing.el ends here
