package main

import (
	"database/sql"
	"encoding/json"
	"fmt"
	"github.com/BurntSushi/toml"
	log "github.com/Sirupsen/logrus"
	"github.com/flosch/pongo2"
	_ "github.com/flosch/pongo2-addons"
	"github.com/go-martini/martini"
	_ "github.com/go-sql-driver/mysql"
	"github.com/yuiolink/yuiolink/utils"
	"math/rand"
	"net/http"
	"strconv"
	"time"
)

type config struct {
	Protocol        string
	Domain          string
	Tls             bool
	TlsCert         string
	TlsKey          string
	BindIp          string
	Port            int
	Database        dbConfig
	ChacheDirectory string
}

type dbConfig struct {
	Host     string
	Port     int
	User     string
	Password string
	Database string
}

type linkContent struct {
	Content     string
	ContentType string
	Encrypted   bool
}

var namespace = []rune("yuphjknm")

func renderHtml(templateName string, context pongo2.Context, writer http.ResponseWriter) {
	var template = pongo2.Must(pongo2.FromCache(templateName))
	template.ExecuteWriter(context, writer)
}

func renderJson(v interface{}, writer http.ResponseWriter) {
	result, err := json.Marshal(v) // TODO: Implement configurable ident

	if err != nil {
		panic(err.Error()) // TODO: Write json error with status code
	}

	writer.Write(result)
}

func main() {
	var conf config
	if _, err := toml.DecodeFile("config.toml", &conf); err != nil {
		panic(err.Error())
	}

	log.Info("Configuration parsed...")
	log.Infof("Configuration values: %s", conf)

	const linkNameLength = 6
	siteRootUrl := utils.BuildRootUrl(conf.Protocol, conf.Domain, conf.Port, conf.Tls)
	log.Infof("Site root URL is set to %s", siteRootUrl)
	connectionString := fmt.Sprintf("%s:%s@tcp(%s:%d)/%s", conf.Database.User, conf.Database.Password, conf.Database.Host, conf.Database.Port, conf.Database.Database)

	rand.Seed(time.Now().UTC().UnixNano())
	m := martini.Classic()
	m.Use(martini.Static("js", martini.StaticOptions{Prefix: "js"}))
	m.Use(martini.Static("css", martini.StaticOptions{Prefix: "css"}))

	m.Get("/", func(response http.ResponseWriter) {
		renderHtml("templates/index.tmpl", nil, response)
	})

	m.Get("/paste", func(response http.ResponseWriter) {
		renderHtml("templates/paste.tmpl", nil, response)
	})

	m.Get("/:linkName", func(params martini.Params, response http.ResponseWriter, request *http.Request) {
		log.Info("Requesting link")

		db, err := sql.Open("mysql", connectionString)
		if err != nil {
			panic(err.Error())
		}
		defer db.Close()

		linkName := params["linkName"]
		var content linkContent
		content, err = GetRedirectFromLinkName(db, linkName)

		if err == nil {
			if content.Encrypted {
				if !utils.IsValidJson(content.Content) {
					panic("Content of link is not valid JSON, possible XSS attack.")
				}

				log.Info("Link is encrypted, serving decryption page")
				renderHtml("templates/encrypted.tmpl", pongo2.Context{"uri": content.Content}, response)
			} else {
				log.Infof("Redirecting to %s", content.Content)
				http.Redirect(response, request, content.Content, 303)
			}
			return
		}

		content, err = GetPasteFromLinkName(db, linkName)
		if err == nil {
			log.Info("Found paste")
			renderHtml("templates/show-paste.tmpl", pongo2.Context{"content": content.Content, "contentType": content.ContentType, "encrypted": content.Encrypted}, response)
			return
		} else {
			panic(err.Error())
		}
	})

	m.Post("/", func(request *http.Request, response http.ResponseWriter) {
		uri := request.FormValue("uri")
		if uri == "" {
			panic("No parameter with key uri provided")
		}

		log.WithFields(log.Fields{
			"uri":       uri,
			"encrypted": false,
		}).Info("Creating link")

		db, err := sql.Open("mysql", connectionString)
		if err != nil {
			panic(err.Error())
		}

		linkName := GenerateUniqueLinkName(db, linkNameLength, namespace)
		log.WithFields(log.Fields{
			"link_name": linkName,
			"uri":       uri,
			"encrypted": false,
		}).Info("Inserting redirect link")
		InsertRedirect(db, linkName, uri, false)

		linkUrl := siteRootUrl + linkName
		renderHtml("templates/index.tmpl", pongo2.Context{"link": linkUrl}, response)
	})

	m.Post("/paste", func(request *http.Request, response http.ResponseWriter) {
		content := request.FormValue("content")
		if content == "" {
			panic("No parameter with key content provided")
		}

		contentType := request.FormValue("content_type")

		db, err := sql.Open("mysql", connectionString)
		if err != nil {
			panic(err.Error())
		}

		linkName := GenerateUniqueLinkName(db, linkNameLength, namespace)
		log.WithFields(log.Fields{
			"link_name": linkName,
		}).Info("Inserting paste link")
		InsertPaste(db, linkName, content, contentType, false)

		linkUrl := siteRootUrl + linkName

		renderHtml("templates/paste.tmpl", pongo2.Context{"link": linkUrl}, response)
	})

	m.Post("/api/redirect", func(request *http.Request, response http.ResponseWriter) string {
		uri := request.FormValue("uri")
		if uri == "" {
			panic("No parameter with key url provided")
		}

		encrypted, err := strconv.ParseBool(request.FormValue("encrypted"))
		if err != nil {
			panic("Invalid value for parameter \"encrypted\"")
		}

		db, err := sql.Open("mysql", connectionString)
		if err != nil {
			panic(err.Error())
		}

		linkName := GenerateUniqueLinkName(db, linkNameLength, namespace)
		log.WithFields(log.Fields{
			"link_name": linkName,
			"uri":       uri,
			"encrypted": encrypted,
		}).Info("Inserting redirect link")
		InsertRedirect(db, linkName, uri, encrypted)

		linkUrl := siteRootUrl + linkName

		return linkUrl
	})

	m.Post("/api/paste", func(request *http.Request, response http.ResponseWriter) string {
		log.Infof("Request: %s", request)

		content := request.FormValue("content")
		if content == "" {
			panic("No parameter with key content provided")
		}

		contentType := request.FormValue("content_type")
		if contentType == "" {
			panic("Invalid value for parameter \"content_type\"")
		}

		encrypted, err := strconv.ParseBool(request.FormValue("encrypted"))
		if err != nil {
			panic("Invalid value for parameter \"encrypted\"")
		}

		db, err := sql.Open("mysql", connectionString)
		if err != nil {
			panic(err.Error())
		}

		linkName := GenerateUniqueLinkName(db, linkNameLength, namespace)
		log.WithFields(log.Fields{
			"link_name": linkName,
			"encrypted": encrypted,
		}).Info("Inserting paste link")
		InsertPaste(db, linkName, content, contentType, encrypted)

		linkUrl := siteRootUrl + linkName

		return linkUrl
	})

	binding := fmt.Sprintf("%s:%d", conf.BindIp, conf.Port)
	if conf.Tls {
		http.ListenAndServeTLS(binding, conf.TlsCert, conf.TlsKey, m)
	} else {
		m.RunOnAddr(binding)
	}
}
