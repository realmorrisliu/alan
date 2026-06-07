;;; alan-project.el --- Project workflow for alan-emacs -*- lexical-binding: t; -*-

;;; Commentary:
;; Built-in project.el and optional built-in Eglot wiring.

;;; Code:

(require 'project)

(global-set-key (kbd "C-c p f") #'project-find-file)
(global-set-key (kbd "C-c p p") #'project-switch-project)
(global-set-key (kbd "C-c p b") #'project-switch-to-buffer)
(global-set-key (kbd "C-c p s") #'project-shell)
(global-set-key (kbd "C-c p d") #'project-dired)

(when (require 'eglot nil t)
  (global-set-key (kbd "C-c l s") #'eglot)
  (global-set-key (kbd "C-c l r") #'eglot-rename)
  (global-set-key (kbd "C-c l a") #'eglot-code-actions))

(provide 'alan-project)
;;; alan-project.el ends here
