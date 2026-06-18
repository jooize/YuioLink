package main

import (
	"database/sql"
	_ "github.com/go-sql-driver/mysql"
	"github.com/yuiolink/yuiolink/utils"
)

func GenerateUniqueLinkName(db *sql.DB, length int, namespace []rune) string {
	var linkName string
	for true {
		linkName = utils.GenerateRandomLinkName(length, namespace)
		if !LinkNameExists(db, linkName) {
			break
		}
	}
	return linkName
}

func GetRedirectLinks(db *sql.DB) (string, string) {
	stmtOut, err := db.Prepare("SELECT l.link, r.redirect_uri FROM link l JOIN redirect r ON r.link_id = l.id")
	if err != nil {
		panic(err.Error())
	}
	defer stmtOut.Close()

	rows, err := stmtOut.Query()
	if err != nil {
		panic(err.Error())
	}

	var uri string
	var link string
	for rows.Next() {
		if err := rows.Scan(&link, &uri); err != nil {
			panic(err.Error())
		}
	}

	return link, uri
}

func LinkNameExists(db *sql.DB, linkName string) bool {
	stmtOut, err := db.Prepare("SELECT EXISTS (SELECT 1 FROM link WHERE link_name = ?)")
	if err != nil {
		panic(err.Error())
	}
	defer stmtOut.Close()

	var result bool
	err = stmtOut.QueryRow(linkName).Scan(&result)
	if err != nil {
		panic(err.Error())
	}

	return result
}

func GetRedirectFromLinkName(db *sql.DB, linkName string) (content linkContent, err error) {
	stmtOut, err := db.Prepare("SELECT r.redirect_uri AS uri, r.encrypted AS encrypted FROM link l JOIN redirect r ON r.link_id = l.id WHERE l.link_name = ?")
	if err != nil {
		panic(err.Error())
	}
	defer stmtOut.Close()

	var uri string
	var encrypted bool
	var redirect linkContent

	err = stmtOut.QueryRow(linkName).Scan(&uri, &encrypted)
	if err != nil {
		return redirect, err
	}

	redirect.Content = uri
	redirect.Encrypted = encrypted

	return redirect, nil
}

func GetPasteFromLinkName(db *sql.DB, linkName string) (p linkContent, err error) {
	stmtOut, err := db.Prepare("SELECT p.content AS content, p.content_type AS contentType, p.encrypted AS encrypted FROM link l JOIN paste p ON p.link_id = l.id WHERE l.link_name = ?")
	if err != nil {
		panic(err.Error())
	}
	defer stmtOut.Close()

	var content string
	var contentType string
	var encrypted bool
	var paste linkContent

	err = stmtOut.QueryRow(linkName).Scan(&content, &contentType, &encrypted)
	if err != nil {
		return paste, err
	}

	paste.Content = content
	paste.ContentType = contentType
	paste.Encrypted = encrypted

	return paste, nil
}

func InsertLink(db *sql.DB, linkName string) int64 {
	linkIns, err := db.Prepare("INSERT INTO link (link_name, date_created) VALUES (?, NOW())")
	if err != nil {
		panic(err.Error())
	}
	defer linkIns.Close()

	result, err := linkIns.Exec(linkName)
	if err != nil {
		panic(err.Error())
	}

	linkId, err := result.LastInsertId()
	if err != nil {
		panic(err.Error())
	}

	return linkId
}

func InsertRedirect(db *sql.DB, linkName string, uri string, encrypted bool) bool {
	linkId := InsertLink(db, linkName)

	redirectIns, err := db.Prepare("INSERT INTO redirect (link_id, redirect_uri, encrypted) VALUES (?, ?, ?)")
	if err != nil {
		panic(err.Error())
	}
	defer redirectIns.Close()

	_, err = redirectIns.Exec(linkId, uri, encrypted)
	if err != nil {
		panic(err.Error())
	}

	return true
}

func InsertPaste(db *sql.DB, linkName string, content string, contentType string, encrypted bool) bool {
	linkId := InsertLink(db, linkName)

	pasteIns, err := db.Prepare("INSERT INTO paste (link_id, content, content_type, encrypted) VALUES (?, ?, ?, ?)")
	if err != nil {
		panic(err.Error())
	}
	defer pasteIns.Close()

	_, err = pasteIns.Exec(linkId, content, contentType, encrypted)
	if err != nil {
		panic(err.Error())
	}

	return true
}
