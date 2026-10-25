#!/usr/bin/env python3
"""Writes ui/sheet.ui: every rendered screen with a one-line design note."""
import os
import sys
from xml.sax.saxutils import escape

design = os.path.abspath(sys.argv[1])
COLUMNS = 5
WIDTH = 330  # logical width of each screenshot on the sheet

SCREENS = [
    ("01-home", "Home", "Answers “do my sites open?” first. Freed hands mean everything works."),
    ("02-home-problem", "A site is blocked", "Chained hands, the reason in plain words, one button: Unblock It."),
    ("10-welcome", "First run", "What Sluis is and isn’t, in three lines. Then straight to unblocking."),
    ("12-phone", "Phone width", "360 px, nothing cut off. Banners only for unusual things like updates."),
    ("13-home-light", "Light style", "Follows the system’s light or dark style and accent colour."),
    ("05-scan-sites", "Unblock Sites · which sites", "Your sites are already filled in; tick others that are often blocked."),
    ("06-scan-progress", "Unblock Sites · looking", "What’s wrong, in plain words, and every method being tried."),
    ("07-scan-result", "Unblock Sites · done", "Double-checked, other sites still work. One button: Turn It On."),
    ("03-strategies", "Methods", "Numbered and ranked, best one already picked. Undo after switching."),
    ("11-keep-strategy", "Automatic switch back", "If a new method breaks a site, Sluis switches back by itself."),
    ("04-strategy-details", "Method details", "Proof for each site. Technical details hidden for experts."),
    ("08-sites", "Which sites", "All sites or only yours, plus sites Sluis should leave alone."),
    ("09-preferences", "Preferences", "Takes effect immediately and searchable; everyday options first."),
    ("09b-preferences-advanced", "Preferences · Advanced", "Expert options; Uninstall alone at the bottom."),
]


def card(name, title, note, col, row):
    path = os.path.join(design, "screens", name + ".png")
    return f"""
        <child>
          <object class="GtkBox">
            <property name="orientation">vertical</property>
            <property name="spacing">4</property>
            <property name="width-request">{WIDTH}</property>
            <child>
              <object class="GtkPicture">
                <property name="file">file://{escape(path)}</property>
                <property name="can-shrink">True</property>
                <property name="content-fit">contain</property>
                <property name="height-request">{int(WIDTH * 1.62)}</property>
                <property name="valign">start</property>
              </object>
            </child>
            <child>
              <object class="GtkLabel">
                <property name="label">{escape(title)}</property>
                <property name="xalign">0</property>
                <property name="margin-start">24</property>
                <style><class name="heading"/></style>
              </object>
            </child>
            <child>
              <object class="GtkLabel">
                <property name="label">{escape(note)}</property>
                <property name="xalign">0</property>
                <property name="wrap">True</property>
                <property name="max-width-chars">34</property>
                <property name="margin-start">24</property>
                <property name="margin-end">12</property>
                <style><class name="dim-label"/><class name="caption"/></style>
              </object>
            </child>
            <layout>
              <property name="column">{col}</property>
              <property name="row">{row}</property>
            </layout>
          </object>
        </child>"""


cards = "".join(card(n, t, d, i % COLUMNS, i // COLUMNS) for i, (n, t, d) in enumerate(SCREENS))
print(f"""<?xml version="1.0" encoding="UTF-8"?>
<interface>
  <object class="GtkWindow" id="window">
    <property name="decorated">False</property>
    <style><class name="sheet"/></style>
    <property name="child">
      <object class="GtkBox">
        <property name="orientation">vertical</property>
        <property name="spacing">6</property>
        <property name="margin-top">40</property>
        <property name="margin-bottom">40</property>
        <property name="margin-start">40</property>
        <property name="margin-end">40</property>
        <child>
          <object class="GtkLabel">
            <property name="label">Sluis 1.0 · Design</property>
            <property name="xalign">0</property>
            <property name="margin-start">24</property>
            <style><class name="title-1"/></style>
          </object>
        </child>
        <child>
          <object class="GtkLabel">
            <property name="label">Real GTK 4.22 + libadwaita 1.9 widgets, stock GNOME style. Placeholder data.</property>
            <property name="xalign">0</property>
            <property name="margin-start">24</property>
            <property name="margin-bottom">16</property>
            <style><class name="dim-label"/></style>
          </object>
        </child>
        <child>
          <object class="GtkBox">
            <property name="spacing">40</property>
            <property name="margin-start">24</property>
            <property name="margin-bottom">28</property>
            <child>
              <object class="GtkBox">
                <property name="spacing">16</property>
                <child><object class="GtkImage"><property name="icon-name">sluis-chained-symbolic</property><property name="pixel-size">64</property><style><class name="hero-icon"/><class name="bad"/></style></object></child>
                <child><object class="GtkLabel"><property name="label">Sites are blocked</property><style><class name="title-4"/></style></object></child>
              </object>
            </child>
            <child>
              <object class="GtkBox">
                <property name="spacing">16</property>
                <child><object class="GtkImage"><property name="icon-name">sluis-freed-symbolic</property><property name="pixel-size">64</property><style><class name="hero-icon"/><class name="ok"/></style></object></child>
                <child><object class="GtkLabel"><property name="label">Sites are unblocked</property><style><class name="title-4"/></style></object></child>
              </object>
            </child>
          </object>
        </child>
        <child>
          <object class="GtkGrid">
            <property name="column-spacing">8</property>
            <property name="row-spacing">28</property>{cards}
          </object>
        </child>
      </object>
    </property>
  </object>
</interface>""")
